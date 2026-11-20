//! `bb commit browse`: list a directory, or print the content of a file.
//!
//! Bitbucket Cloud serves the two cases from one endpoint,
//! `GET /2.0/repositories/{workspace}/{repo_slug}/src/{commit}/{path}`. A
//! directory gives a JSON page of entries. A file gives the raw bytes of the
//! file. The command asks for `?format=meta` first, which gives a JSON object
//! with a `type`, and then sends the request that fits the type.
//!
//! The endpoint takes one path segment as the revision. A branch or tag name
//! that contains `/` is resolved to its commit hash first, through
//! `refs/branches/{name}` and then `refs/tags/{name}`.
//!
//! The content of a file is never converted. The bytes that the server sends
//! are the bytes that reach stdout or the output file.

use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context, Result};
use atlassian_cli_api::error::ApiError;
use atlassian_cli_api::path::encode_path_segment;
use atlassian_cli_api::RawRequest;
use reqwest::header::HeaderMap;
use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::utils::{encode_ref_path, BitbucketContext};
use crate::commands::common::{render_success, write_bytes, MutationResult};

/// `type` of a file in the `format=meta` response.
const TYPE_FILE: &str = "commit_file";
/// `type` of a directory in the `format=meta` response.
const TYPE_DIRECTORY: &str = "commit_directory";

#[derive(Deserialize)]
struct SourceFile {
    path: String,
    #[serde(rename = "type")]
    file_type: String,
    #[serde(default)]
    size: Option<i64>,
}

#[derive(Deserialize)]
struct SourceList {
    values: Vec<SourceFile>,
}

/// The `format=meta` object. Only `type` is read.
#[derive(Deserialize)]
struct SourceMeta {
    #[serde(rename = "type")]
    entry_type: String,
}

/// A branch or a tag from `refs/branches/{name}` or `refs/tags/{name}`. Only
/// the hash of the target commit is read.
#[derive(Deserialize)]
struct RefObject {
    target: RefTarget,
}

#[derive(Deserialize)]
struct RefTarget {
    hash: String,
}

/// One row of a directory list. The keys are the output keys of the command.
#[derive(Debug, Serialize, PartialEq)]
struct SourceRow {
    file_type: String,
    path: String,
    size: String,
}

/// What a browse request found.
#[derive(Debug, PartialEq)]
enum Source {
    /// The path is a file. These are the bytes that the server sent.
    File(Vec<u8>),
    /// The path is a directory, or no path was given.
    Directory(Vec<SourceRow>),
}

/// What the caller asked `bb commit browse` to do.
pub struct BrowseRequest<'a> {
    pub commit: &'a str,
    pub path: Option<&'a str>,
    /// `--output`: `-` is stdout, any other value is a file to write.
    pub output: Option<&'a Path>,
    /// `--force`: replace an existing output file.
    pub force: bool,
}

/// Encodes a repository path for the request path.
///
/// `/` stays as the separator and each segment is percent-encoded. Leading and
/// trailing `/` are removed, so `src/` and `/src` address the same directory.
/// `None` means the repository root. An empty segment, `.` and `..` are
/// refused, because the URL parser would resolve them to a different resource.
fn encode_source_path(path: &str) -> Result<Option<String>> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(None);
    }
    let segments = trimmed
        .split('/')
        .map(encode_path_segment)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("Invalid path {path:?}"))?;
    Ok(Some(segments.join("/")))
}

/// Adds the probable cause to a 404, and the request summary to all errors.
fn browse_error(
    err: ApiError,
    what: &str,
    request: &BrowseRequest<'_>,
    workspace: &str,
    repo_slug: &str,
) -> anyhow::Error {
    let repo = format!("{workspace}/{repo_slug}");
    let commit = request.commit;
    let shown_path = request.path.unwrap_or("/");
    if matches!(err, ApiError::NotFound { .. }) {
        return anyhow::Error::new(err).context(format!(
            "No file or directory {shown_path:?} at revision {commit:?} in {repo}. \
             Check the path, the revision and the repository."
        ));
    }
    anyhow::Error::new(err).context(format!(
        "Failed to {what} {shown_path:?} at revision {commit:?} in {repo}"
    ))
}

/// The request path of a browse target, and what the target is.
#[derive(Debug, PartialEq)]
enum Target {
    File(String),
    Directory(String),
    /// A directory whose list is already here (see the 404 path of
    /// [`resolve_target`]).
    Listed(Vec<SourceRow>),
}

/// Gives the revision for the `src` request path.
///
/// A revision with no `/` (a commit hash, or a simple branch or tag name) is
/// used as it is, and no request is sent. The `src` endpoint reads one path
/// segment as the revision, so a name with `/` cannot go into that path. Such
/// a name is resolved to the hash of its target commit: first as a branch,
/// then as a tag if the branch request gives a 404.
async fn resolve_revision(
    ctx: &BitbucketContext<'_>,
    repo_base: &str,
    request: &BrowseRequest<'_>,
    workspace: &str,
    repo_slug: &str,
) -> Result<String> {
    let commit = request.commit;
    let name = encode_ref_path(commit)?;
    if !commit.contains('/') {
        return Ok(name);
    }

    for kind in ["branches", "tags"] {
        let ref_path = format!("{repo_base}/refs/{kind}/{name}");
        match ctx.client.get::<RefObject>(&ref_path).await {
            Ok(found) => {
                tracing::debug!(
                    revision = commit,
                    kind,
                    hash = %found.target.hash,
                    "Resolved the revision to a commit hash"
                );
                return encode_path_segment(&found.target.hash).with_context(|| {
                    format!("Bitbucket gave an invalid commit hash for revision {commit:?}")
                });
            }
            Err(ApiError::NotFound { .. }) => {
                tracing::debug!(
                    revision = commit,
                    kind,
                    "The revision is not in this ref kind"
                );
            }
            Err(err) => {
                return Err(anyhow::Error::new(err).context(format!(
                    "Failed to resolve revision {commit:?} in {workspace}/{repo_slug}"
                )))
            }
        }
    }
    bail!(
        "Revision {commit:?} is not a branch or a tag in {workspace}/{repo_slug}. A revision \
         that contains '/' must be a branch name or a tag name. Check the name and the \
         repository."
    )
}

/// The `error.message` text of a Bitbucket error body, if the body has one.
fn server_message(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let message = value.get("error")?.get("message")?.as_str()?.trim();
    (!message.is_empty()).then(|| message.to_string())
}

/// The step after a 404 from the meta request.
///
/// The typed client keeps no body for a 404, so that error cannot say if the
/// commit or the path is the cause. This sends the list request without the
/// status mapping and reads the body. If the answer is a directory list, the
/// command uses it. If not, the error has the message that Bitbucket sent.
///
/// The list part is defensive. The Bitbucket API description shows the meta
/// link of a directory with a trailing slash, and does not state what a meta
/// request with no trailing slash gives.
async fn list_after_meta_404(
    ctx: &BitbucketContext<'_>,
    api_path: &str,
    meta_err: ApiError,
    request: &BrowseRequest<'_>,
    workspace: &str,
    repo_slug: &str,
) -> Result<Target> {
    tracing::debug!("The meta request gave a 404, sending the list request");
    let response = match ctx
        .client
        .request_raw(RawRequest {
            method: Method::GET,
            path: api_path,
            headers: HeaderMap::new(),
            body: None,
            timeout: None,
        })
        .await
    {
        Ok(response) => response,
        Err(list_err) => {
            tracing::debug!(error = %list_err, "The list request failed also");
            return Err(browse_error(
                meta_err, "find", request, workspace, repo_slug,
            ));
        }
    };

    if response.is_success() {
        if let Ok(list) = serde_json::from_slice::<SourceList>(&response.body) {
            return Ok(Target::Listed(directory_rows(list)));
        }
        return Err(browse_error(
            meta_err, "find", request, workspace, repo_slug,
        ));
    }

    let error = browse_error(meta_err, "find", request, workspace, repo_slug);
    match server_message(&response.body) {
        Some(message) => {
            tracing::debug!(status = response.status, %message, "The list request failed also");
            Err(error.context(format!("Bitbucket says: {message}")))
        }
        None => Err(error),
    }
}

/// Finds what the path is, without a read of its content.
///
/// No path is the repository root, which is always a directory, so that case
/// sends no `src` request here. Each other path costs one `format=meta`
/// request. The Content-Type of the content response is not used: Bitbucket
/// derives it from the file name, so a `.json` file looks the same as a
/// directory list. A 404 from the meta request goes to
/// [`list_after_meta_404`].
async fn resolve_target(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    request: &BrowseRequest<'_>,
) -> Result<Target> {
    let repo_base = format!(
        "/2.0/repositories/{}/{}",
        encode_path_segment(workspace).context("Invalid workspace")?,
        encode_path_segment(repo_slug).context("Invalid repository")?,
    );
    // The path is checked before the revision, so that a refused path sends no
    // request.
    let encoded_path = encode_source_path(request.path.unwrap_or(""))?;
    let revision = resolve_revision(ctx, &repo_base, request, workspace, repo_slug).await?;
    let base = format!("{repo_base}/src/{revision}");
    let Some(encoded_path) = encoded_path else {
        // The trailing slash is necessary for the root directory.
        return Ok(Target::Directory(format!("{base}/")));
    };
    let api_path = format!("{base}/{encoded_path}");

    let meta: SourceMeta = match ctx.client.get(&format!("{api_path}?format=meta")).await {
        Ok(meta) => meta,
        Err(err @ ApiError::NotFound { .. }) => {
            return list_after_meta_404(ctx, &api_path, err, request, workspace, repo_slug).await;
        }
        Err(err) => return Err(browse_error(err, "find", request, workspace, repo_slug)),
    };
    tracing::debug!(
        path = request.path.unwrap_or("/"),
        commit = request.commit,
        entry_type = %meta.entry_type,
        "Resolved the type of the browse target"
    );

    match meta.entry_type.as_str() {
        TYPE_FILE => Ok(Target::File(api_path)),
        TYPE_DIRECTORY => Ok(Target::Directory(api_path)),
        other => bail!(
            "Bitbucket reports the type {other:?} for {:?}. The command can read \
             {TYPE_FILE} and {TYPE_DIRECTORY} only.",
            request.path.unwrap_or("/")
        ),
    }
}

async fn fetch_target(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    request: &BrowseRequest<'_>,
    target: Target,
) -> Result<Source> {
    match target {
        Target::Listed(rows) => Ok(Source::Directory(rows)),
        Target::File(api_path) => {
            let bytes = ctx
                .client
                .get_bytes(&api_path)
                .await
                .map_err(|err| browse_error(err, "read", request, workspace, repo_slug))?;
            tracing::debug!(bytes = bytes.len(), "Fetched the file content");
            Ok(Source::File(bytes))
        }
        Target::Directory(api_path) => {
            let list: SourceList = ctx
                .client
                .get(&api_path)
                .await
                .map_err(|err| browse_error(err, "list", request, workspace, repo_slug))?;
            Ok(Source::Directory(directory_rows(list)))
        }
    }
}

#[cfg(test)]
async fn fetch_source(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    request: &BrowseRequest<'_>,
) -> Result<Source> {
    let target = resolve_target(ctx, workspace, repo_slug, request).await?;
    fetch_target(ctx, workspace, repo_slug, request, target).await
}

fn directory_rows(list: SourceList) -> Vec<SourceRow> {
    list.values
        .into_iter()
        .map(|file| SourceRow {
            file_type: file.file_type,
            path: file.path,
            size: file.size.map(|s| format!("{s} bytes")).unwrap_or_default(),
        })
        .collect()
}

/// Writes the content of a file to `out`, with no change and no added newline.
///
/// A broken pipe is not an error: in `bb commit browse ... | head -1` the
/// reader closes the pipe when it has what it needs.
fn write_file_content(out: &mut dyn Write, bytes: &[u8]) -> Result<()> {
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
            tracing::debug!("The reader closed stdout before the end of the content");
            Ok(())
        }
        Err(err) => Err(err).context("Failed to write the file content to stdout"),
    }
}

/// `bb commit browse`. See [`browse_source_to`].
pub async fn browse_source(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    request: &BrowseRequest<'_>,
) -> Result<()> {
    browse_source_to(ctx, workspace, repo_slug, request, &mut std::io::stdout()).await
}

/// Lists a directory through the renderer, or sends the content of a file to
/// `out` (stdout in the command) or to the `--output` file.
///
/// In file mode with no output file, `out` gets the file bytes and nothing
/// else, for every output format. Logs go to stderr.
async fn browse_source_to(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    request: &BrowseRequest<'_>,
    out: &mut (dyn Write + Send),
) -> Result<()> {
    let to_file = request.output.filter(|p| *p != Path::new("-"));

    // An early check, so that a refused target costs no request. The real
    // guard is `create_new` in `write_bytes`. `symlink_metadata` also finds a
    // symbolic link that points to nothing.
    if let Some(file) = to_file {
        if !request.force && file.symlink_metadata().is_ok() {
            bail!(
                "{} already exists. Use --force to overwrite.",
                file.display()
            );
        }
    }

    let target = resolve_target(ctx, workspace, repo_slug, request).await?;
    if let (Target::Directory(_) | Target::Listed(_), Some(file)) = (&target, to_file) {
        bail!(
            "{:?} is a directory. --output {} applies to a file path only.",
            request.path.unwrap_or("/"),
            file.display()
        );
    }

    match fetch_target(ctx, workspace, repo_slug, request, target).await? {
        Source::File(bytes) => {
            let shown_path = request.path.unwrap_or("/");
            match to_file {
                None => write_file_content(out, &bytes),
                Some(file) => {
                    write_bytes(file, &bytes, request.force)?;
                    tracing::debug!(
                        path = shown_path,
                        file = %file.display(),
                        bytes = bytes.len(),
                        "File content written"
                    );
                    let message = format!(
                        "Wrote {} bytes of {shown_path} to {}",
                        bytes.len(),
                        file.display()
                    );
                    render_success(
                        ctx.renderer,
                        &format!("✅ {message}"),
                        &MutationResult::new(message.clone()),
                    )
                }
            }
        }
        Source::Directory(rows) => {
            if rows.is_empty() {
                tracing::info!(
                    commit = request.commit,
                    path = request.path.unwrap_or("/"),
                    workspace,
                    repo_slug,
                    "No files found"
                );
            }
            ctx.renderer.render_list_or_empty(&rows, "No files found")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlassian_cli_api::ApiClient;
    use atlassian_cli_output::{OutputFormat, OutputRenderer};
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Bytes that are not valid UTF-8, with a NUL and no final newline.
    const BINARY: &[u8] = &[0xff, 0xfe, 0x00, b'a', b'\n', 0x80, 0xc3];

    fn ctx<'a>(server: &MockServer, renderer: &'a OutputRenderer) -> BitbucketContext<'a> {
        BitbucketContext {
            client: ApiClient::new(server.uri()).unwrap(),
            renderer,
            is_bearer: false,
        }
    }

    fn request<'a>(commit: &'a str, path: Option<&'a str>) -> BrowseRequest<'a> {
        BrowseRequest {
            commit,
            path,
            output: None,
            force: false,
        }
    }

    /// Mounts the `format=meta` answer for `url_path`.
    async fn mount_meta(server: &MockServer, url_path: &str, entry_type: &str) {
        Mock::given(method("GET"))
            .and(path(url_path))
            .and(query_param("format", "meta"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "type": entry_type, "path": "x" })),
            )
            .mount(server)
            .await;
    }

    /// Mounts the raw content of a file at `url_path`.
    async fn mount_file(server: &MockServer, url_path: &str, bytes: &[u8], mime: &str) {
        Mock::given(method("GET"))
            .and(path(url_path))
            .and(query_param_is_missing("format"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.to_vec(), mime))
            .mount(server)
            .await;
    }

    async fn mount_directory(server: &MockServer, url_path: &str) {
        Mock::given(method("GET"))
            .and(path(url_path))
            .and(query_param_is_missing("format"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "pagelen": 10,
                "values": [
                    { "path": "src/lib", "type": "commit_directory" },
                    { "path": "src/main.rs", "type": "commit_file", "size": 120 }
                ]
            })))
            .mount(server)
            .await;
    }

    /// Path and query of each request that the server received.
    async fn received(server: &MockServer) -> Vec<String> {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| match r.url.query() {
                Some(q) => format!("{}?{q}", r.url.path()),
                None => r.url.path().to_string(),
            })
            .collect()
    }

    fn expected_rows() -> Vec<SourceRow> {
        vec![
            SourceRow {
                file_type: "commit_directory".into(),
                path: "src/lib".into(),
                size: String::new(),
            },
            SourceRow {
                file_type: "commit_file".into(),
                path: "src/main.rs".into(),
                size: "120 bytes".into(),
            },
        ]
    }

    #[tokio::test]
    async fn file_path_sends_the_exact_bytes_to_stdout_in_each_format() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/assets/logo.bin";
        for format in [
            OutputFormat::Table,
            OutputFormat::Json,
            OutputFormat::Yaml,
            OutputFormat::Csv,
            OutputFormat::Quiet,
        ] {
            let server = MockServer::start().await;
            mount_meta(&server, url, TYPE_FILE).await;
            mount_file(&server, url, BINARY, "application/octet-stream").await;
            let renderer = OutputRenderer::new(format);
            let mut out: Vec<u8> = Vec::new();

            browse_source_to(
                &ctx(&server, &renderer),
                "myworkspace",
                "web_app",
                &request("main", Some("assets/logo.bin")),
                &mut out,
            )
            .await
            .unwrap();

            assert_eq!(out, BINARY, "stdout must be the file bytes for {format:?}");
        }
    }

    // A `.json` file comes back as `application/json`, and it can have a
    // `values` key. It is a file, so the command must not list it.
    #[tokio::test]
    async fn json_file_with_a_values_key_is_content_not_a_list() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/data.json";
        let body = br#"{"values":[{"path":"a","type":"commit_file"}]}"#;
        let server = MockServer::start().await;
        mount_meta(&server, url, TYPE_FILE).await;
        mount_file(&server, url, body, "application/json").await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let mut out: Vec<u8> = Vec::new();

        browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("main", Some("data.json")),
            &mut out,
        )
        .await
        .unwrap();

        assert_eq!(out, body);
    }

    #[tokio::test]
    async fn empty_file_writes_nothing() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/empty.txt";
        let server = MockServer::start().await;
        mount_meta(&server, url, TYPE_FILE).await;
        mount_file(&server, url, b"", "text/plain").await;
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let mut out: Vec<u8> = Vec::new();

        browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("main", Some("empty.txt")),
            &mut out,
        )
        .await
        .unwrap();

        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn directory_path_lists_the_entries_and_writes_no_file_bytes() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/src";
        for format in [OutputFormat::Table, OutputFormat::Json] {
            let server = MockServer::start().await;
            mount_meta(&server, url, TYPE_DIRECTORY).await;
            mount_directory(&server, url).await;
            let renderer = OutputRenderer::new(format);
            let context = ctx(&server, &renderer);
            let browse = request("main", Some("src"));

            let source = fetch_source(&context, "myworkspace", "web_app", &browse)
                .await
                .unwrap();
            let Source::Directory(rows) = source else {
                panic!("a directory path must give rows");
            };
            assert_eq!(rows, expected_rows());
            // These rows go to the renderer. Their JSON keys are part of the
            // output contract.
            assert_eq!(
                serde_json::to_value(&rows).unwrap(),
                json!([
                    { "file_type": "commit_directory", "path": "src/lib", "size": "" },
                    { "file_type": "commit_file", "path": "src/main.rs", "size": "120 bytes" }
                ])
            );

            // The list goes through the renderer, not through `out`.
            let mut out: Vec<u8> = Vec::new();
            browse_source_to(&context, "myworkspace", "web_app", &browse, &mut out)
                .await
                .unwrap();
            assert!(out.is_empty(), "no raw bytes for a directory in {format:?}");
        }
    }

    #[tokio::test]
    async fn no_path_lists_the_root_with_one_request() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/";
        let server = MockServer::start().await;
        mount_directory(&server, url).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let context = ctx(&server, &renderer);

        for root in [None, Some("/"), Some("")] {
            let source = fetch_source(&context, "myworkspace", "web_app", &request("main", root))
                .await
                .unwrap();
            assert_eq!(source, Source::Directory(expected_rows()));
        }
        assert_eq!(received(&server).await, vec![url.to_string(); 3]);
    }

    #[tokio::test]
    async fn path_with_a_space_and_nested_directories_is_encoded_by_segment() {
        let url =
            "/2.0/repositories/myworkspace/web_app/src/main/docs/user%20guide/read%20me%231.md";
        let server = MockServer::start().await;
        mount_meta(&server, url, TYPE_FILE).await;
        mount_file(&server, url, b"hello", "text/markdown").await;
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let mut out: Vec<u8> = Vec::new();

        browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("main", Some("docs/user guide/read me#1.md")),
            &mut out,
        )
        .await
        .unwrap();

        assert_eq!(out, b"hello");
        assert_eq!(
            received(&server).await,
            vec![format!("{url}?format=meta"), url.to_string()]
        );
    }

    const HASH: &str = "1a2b3c4d5e6f";
    const BRANCH_REF: &str = "/2.0/repositories/myworkspace/web_app/refs/branches/feature/login";
    const TAG_REF: &str = "/2.0/repositories/myworkspace/web_app/refs/tags/feature/login";

    /// Mounts a ref answer (`target.hash`), with the number of calls expected.
    async fn mount_ref(server: &MockServer, url_path: &str, status: u16, calls: u64) {
        let template = if status == 200 {
            ResponseTemplate::new(200).set_body_json(json!({
                "name": "feature/login",
                "target": { "type": "commit", "hash": HASH }
            }))
        } else {
            ResponseTemplate::new(status).set_body_json(json!({
                "type": "error",
                "error": { "message": "No such ref" }
            }))
        };
        Mock::given(method("GET"))
            .and(path(url_path))
            .respond_with(template)
            .expect(calls)
            .mount(server)
            .await;
    }

    // Bitbucket reads one path segment as the revision, so `feature/login`
    // cannot go into the `src` path. The command asks for the branch first.
    #[tokio::test]
    async fn revision_with_a_slash_is_resolved_to_the_branch_hash() {
        let url = format!("/2.0/repositories/myworkspace/web_app/src/{HASH}/README.md");
        let server = MockServer::start().await;
        mount_ref(&server, BRANCH_REF, 200, 1).await;
        mount_ref(&server, TAG_REF, 200, 0).await;
        mount_meta(&server, &url, TYPE_FILE).await;
        mount_file(&server, &url, b"readme", "text/markdown").await;
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let mut out: Vec<u8> = Vec::new();

        browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("feature/login", Some("README.md")),
            &mut out,
        )
        .await
        .unwrap();

        assert_eq!(out, b"readme");
        assert_eq!(
            received(&server).await,
            vec![BRANCH_REF.to_string(), format!("{url}?format=meta"), url]
        );
    }

    #[tokio::test]
    async fn revision_with_a_slash_is_resolved_to_the_tag_hash_when_no_branch_exists() {
        let url = format!("/2.0/repositories/myworkspace/web_app/src/{HASH}/README.md");
        let server = MockServer::start().await;
        mount_ref(&server, BRANCH_REF, 404, 1).await;
        mount_ref(&server, TAG_REF, 200, 1).await;
        mount_meta(&server, &url, TYPE_FILE).await;
        mount_file(&server, &url, b"readme", "text/markdown").await;
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let mut out: Vec<u8> = Vec::new();

        browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("feature/login", Some("README.md")),
            &mut out,
        )
        .await
        .unwrap();

        assert_eq!(out, b"readme");
    }

    #[tokio::test]
    async fn revision_with_a_slash_that_is_not_a_branch_or_a_tag_is_an_error() {
        let server = MockServer::start().await;
        mount_ref(&server, BRANCH_REF, 404, 1).await;
        mount_ref(&server, TAG_REF, 404, 1).await;
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let mut out: Vec<u8> = Vec::new();

        let err = browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("feature/login", Some("README.md")),
            &mut out,
        )
        .await
        .unwrap_err();

        let text = format!("{err:#}");
        assert!(text.contains("\"feature/login\""), "got {text}");
        assert!(text.contains("not a branch or a tag"), "got {text}");
        // No `src` request follows.
        assert_eq!(
            received(&server).await,
            vec![BRANCH_REF.to_string(), TAG_REF.to_string()]
        );
    }

    #[tokio::test]
    async fn revision_with_no_slash_makes_no_refs_request() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/README.md";
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex("/refs/"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        mount_meta(&server, url, TYPE_FILE).await;
        mount_file(&server, url, b"readme", "text/markdown").await;
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let mut out: Vec<u8> = Vec::new();

        browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("main", Some("README.md")),
            &mut out,
        )
        .await
        .unwrap();

        assert_eq!(out, b"readme");
    }

    // The 404 of the meta request has no text of use. The list request that
    // follows carries the cause, and the user must see it.
    #[tokio::test]
    async fn not_found_error_carries_the_message_of_the_server() {
        for cause in ["Commit not found", "No such file or directory: nope.txt"] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(query_param("format", "meta"))
                .respond_with(ResponseTemplate::new(404))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(query_param_is_missing("format"))
                .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                    "type": "error",
                    "error": { "message": cause }
                })))
                .mount(&server)
                .await;
            let renderer = OutputRenderer::new(OutputFormat::Json);
            let mut out: Vec<u8> = Vec::new();

            let err = browse_source_to(
                &ctx(&server, &renderer),
                "myworkspace",
                "web_app",
                &request("main", Some("nope.txt")),
                &mut out,
            )
            .await
            .unwrap_err();

            let text = format!("{err:#}");
            assert!(text.contains(cause), "got {text}");
            assert!(
                text.contains("No file or directory \"nope.txt\" at revision \"main\""),
                "got {text}"
            );
        }
    }

    #[tokio::test]
    async fn existing_output_file_without_force_sends_no_request() {
        let server = MockServer::start().await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.txt");
        std::fs::write(&target, b"old").unwrap();
        let mut out: Vec<u8> = Vec::new();
        let mut browse = request("main", Some("a.txt"));
        browse.output = Some(&target);

        let err = browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &browse,
            &mut out,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("already exists"), "got {err}");
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert!(received(&server).await.is_empty());
    }

    /// A reader that went away, as in `bb commit browse ... | head -1`.
    struct ClosedPipe;

    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }

    /// A writer that fails for a different cause.
    struct FullDisk;

    impl Write for FullDisk {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::StorageFull.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn broken_pipe_on_stdout_is_not_an_error() {
        assert!(write_file_content(&mut ClosedPipe, b"content").is_ok());
        assert!(write_file_content(&mut FullDisk, b"content").is_err());
    }

    #[tokio::test]
    async fn dot_segments_and_bad_identifiers_send_no_request() {
        let server = MockServer::start().await;
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let context = ctx(&server, &renderer);

        let cases = [
            ("myworkspace", "web_app", "main", Some("src/../../other")),
            ("myworkspace", "web_app", "main", Some("src//main.rs")),
            ("myworkspace", "web_app", "main", Some("./main.rs")),
            ("myworkspace", "web_app", "../main", Some("main.rs")),
            ("myworkspace", "web_app/x", "main", Some("main.rs")),
            ("", "web_app", "main", Some("main.rs")),
        ];
        for (workspace, repo, commit, file) in cases {
            let mut out: Vec<u8> = Vec::new();
            let result =
                browse_source_to(&context, workspace, repo, &request(commit, file), &mut out).await;
            assert!(result.is_err(), "{workspace}/{repo} {commit} {file:?}");
            assert!(out.is_empty());
        }
        assert!(received(&server).await.is_empty());
    }

    #[tokio::test]
    async fn missing_path_gives_an_error_that_names_the_path_and_revision() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "error",
                "error": { "message": "No such file or directory: nope.txt" }
            })))
            .mount(&server)
            .await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let mut out: Vec<u8> = Vec::new();

        let err = browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("main", Some("nope.txt")),
            &mut out,
        )
        .await
        .unwrap_err();

        let text = format!("{err:#}");
        assert!(
            text.contains("No file or directory \"nope.txt\" at revision \"main\""),
            "got {text}"
        );
        assert!(text.contains("myworkspace/web_app"), "got {text}");
        assert!(!text.contains("Invalid response format"), "got {text}");
        assert!(out.is_empty());
    }

    // The document shows the meta link of a directory with a trailing slash.
    // If Bitbucket gives a 404 to the form with no slash, the list must work
    // as it did before this change.
    #[tokio::test]
    async fn directory_lists_when_the_meta_request_gives_a_404() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/src";
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(url))
            .and(query_param("format", "meta"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        mount_directory(&server, url).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);

        let source = fetch_source(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("main", Some("src")),
        )
        .await
        .unwrap();

        assert_eq!(source, Source::Directory(expected_rows()));
        assert_eq!(
            received(&server).await,
            vec![format!("{url}?format=meta"), url.to_string()]
        );
    }

    #[tokio::test]
    async fn output_file_gets_the_bytes_and_stdout_gets_none_of_them() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/assets/logo.bin";
        let server = MockServer::start().await;
        mount_meta(&server, url, TYPE_FILE).await;
        mount_file(&server, url, BINARY, "application/octet-stream").await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("logo.bin");
        let mut out: Vec<u8> = Vec::new();
        let mut browse = request("main", Some("assets/logo.bin"));
        browse.output = Some(&target);

        browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &browse,
            &mut out,
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), BINARY);
        assert!(out.is_empty(), "the bytes go to the file only");
    }

    #[tokio::test]
    async fn output_file_is_not_replaced_without_force() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/a.txt";
        let server = MockServer::start().await;
        mount_meta(&server, url, TYPE_FILE).await;
        mount_file(&server, url, b"new", "text/plain").await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let context = ctx(&server, &renderer);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.txt");
        std::fs::write(&target, b"old").unwrap();
        let mut out: Vec<u8> = Vec::new();
        let mut browse = request("main", Some("a.txt"));
        browse.output = Some(&target);

        let err = browse_source_to(&context, "myworkspace", "web_app", &browse, &mut out)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already exists"), "got {err}");
        assert_eq!(std::fs::read(&target).unwrap(), b"old");

        browse.force = true;
        browse_source_to(&context, "myworkspace", "web_app", &browse, &mut out)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    #[tokio::test]
    async fn output_dash_is_stdout() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/a.txt";
        let server = MockServer::start().await;
        mount_meta(&server, url, TYPE_FILE).await;
        mount_file(&server, url, BINARY, "text/plain").await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let mut out: Vec<u8> = Vec::new();
        let mut browse = request("main", Some("a.txt"));
        browse.output = Some(Path::new("-"));

        browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &browse,
            &mut out,
        )
        .await
        .unwrap();

        assert_eq!(out, BINARY);
    }

    #[tokio::test]
    async fn output_file_with_a_directory_path_is_an_error_and_writes_no_file() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/src";
        let server = MockServer::start().await;
        mount_meta(&server, url, TYPE_DIRECTORY).await;
        mount_directory(&server, url).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.txt");
        let mut out: Vec<u8> = Vec::new();
        let mut browse = request("main", Some("src"));
        browse.output = Some(&target);

        let err = browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &browse,
            &mut out,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("is a directory"), "got {err}");
        assert!(!target.exists());
        // The meta request is sufficient to refuse. No list request follows.
        assert_eq!(received(&server).await, vec![format!("{url}?format=meta")]);
    }

    #[tokio::test]
    async fn unknown_meta_type_is_an_error() {
        let url = "/2.0/repositories/myworkspace/web_app/src/main/odd";
        let server = MockServer::start().await;
        mount_meta(&server, url, "commit_other").await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let mut out: Vec<u8> = Vec::new();

        let err = browse_source_to(
            &ctx(&server, &renderer),
            "myworkspace",
            "web_app",
            &request("main", Some("odd")),
            &mut out,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("commit_other"), "got {err}");
    }

    #[test]
    fn encode_source_path_rules() {
        assert_eq!(encode_source_path("").unwrap(), None);
        assert_eq!(encode_source_path("/").unwrap(), None);
        assert_eq!(encode_source_path("src/").unwrap().as_deref(), Some("src"));
        assert_eq!(
            encode_source_path("/a b/c?d/e%20f").unwrap().as_deref(),
            Some("a%20b/c%3Fd/e%2520f")
        );
        assert!(encode_source_path("a/../b").is_err());
        assert!(encode_source_path("a//b").is_err());
    }
}
