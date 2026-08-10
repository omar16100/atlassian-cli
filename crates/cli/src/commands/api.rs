//! Raw authenticated API passthrough, in the spirit of `gh api` / `glab api`.
//!
//! Product-agnostic on purpose: every product's `execute` already receives the
//! same `(ApiClient, &OutputRenderer)` pair, and `main` already owns the
//! per-product client construction. Only the clap wiring is per-product.
//!
//! Output does NOT go through `OutputRenderer::render`. Rendering a raw API dump
//! as a table would silently drop nested fields, so the body is emitted as-is
//! (pretty-printed when it is JSON) and `--format table/csv/markdown/quiet` are
//! ignored.

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use atlassian_cli_api::{ApiClient, RawRequest, RawResponse};
use atlassian_cli_output::{OutputFormat, OutputRenderer};
use clap::{Args, ValueEnum};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl From<HttpMethod> for Method {
    fn from(value: HttpMethod) -> Self {
        match value {
            HttpMethod::Get => Method::GET,
            HttpMethod::Post => Method::POST,
            HttpMethod::Put => Method::PUT,
            HttpMethod::Patch => Method::PATCH,
            HttpMethod::Delete => Method::DELETE,
            HttpMethod::Head => Method::HEAD,
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct ApiArgs {
    /// Endpoint path, e.g. /rest/api/3/myself. Must resolve to the profile's own site
    pub path: String,

    /// HTTP method. Defaults to GET, or POST when --data is given
    #[arg(short = 'X', long, value_enum)]
    pub method: Option<HttpMethod>,

    /// Request body: inline JSON, @file, or - to read stdin
    #[arg(short = 'd', long, allow_hyphen_values = true)]
    pub data: Option<String>,

    /// Extra request header as "Name: value" (repeatable)
    #[arg(short = 'H', long = "header")]
    pub headers: Vec<String>,

    /// Query parameter as key=value, percent-encoded for you (repeatable)
    #[arg(long = "query")]
    pub queries: Vec<String>,

    /// Print the status line and response headers before the body
    #[arg(short = 'i', long)]
    pub include: bool,

    /// Write the response body to a file instead of stdout
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Print the resolved request and exit without sending it
    #[arg(long)]
    pub dry_run: bool,

    /// Required to send a DELETE
    #[arg(long)]
    pub force: bool,

    /// Per-request timeout in seconds, overriding the 30s default
    #[arg(long)]
    pub timeout: Option<u64>,
}

/// Everything argument parsing produces, independent of clap. This is the seam
/// the unit tests drive.
#[derive(Debug)]
pub(crate) struct ResolvedRequest {
    pub method: Method,
    pub path: String,
    pub headers: HeaderMap,
    pub body: Option<Vec<u8>>,
}

/// A body of `-` or `@file` is resolved here, hence the injected reader.
pub(crate) fn resolve(args: &ApiArgs, stdin: &mut impl Read) -> Result<ResolvedRequest> {
    let body = match args.data.as_deref() {
        Some(spec) => Some(read_body(spec, stdin)?),
        None => None,
    };
    let method = default_method(args.method, body.is_some());
    let mut headers = parse_headers(&args.headers)?;

    // Only default the content type when a body is present and the user has not
    // chosen one; Atlassian APIs are JSON, but -H must still win.
    if body.is_some() && !headers.contains_key(reqwest::header::CONTENT_TYPE) {
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }

    Ok(ResolvedRequest {
        method,
        path: append_queries(&args.path, &args.queries)?,
        headers,
        body,
    })
}

/// GET with no body, POST with one. Matches gh/glab, and avoids the trap where a
/// body is silently dropped because the method defaulted to GET.
fn default_method(explicit: Option<HttpMethod>, has_body: bool) -> Method {
    match explicit {
        Some(m) => m.into(),
        None if has_body => Method::POST,
        None => Method::GET,
    }
}

fn read_body(spec: &str, stdin: &mut impl Read) -> Result<Vec<u8>> {
    // The bytes go on the wire verbatim: no parse-and-reserialize, because some
    // endpoints care about the exact payload and users expect what they typed.
    match spec {
        "-" => {
            let mut buf = Vec::new();
            stdin
                .read_to_end(&mut buf)
                .context("Failed to read stdin")?;
            Ok(buf)
        }
        s if s.starts_with('@') => {
            let path = &s[1..];
            std::fs::read(path).with_context(|| format!("Failed to read body file: {path}"))
        }
        s => Ok(s.as_bytes().to_vec()),
    }
}

fn parse_headers(raw: &[String]) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for entry in raw {
        let (name, value) = entry
            .split_once(':')
            .with_context(|| format!("Invalid --header '{entry}': expected 'Name: value'"))?;
        let name = name.trim();
        let value = value.trim();

        if name.eq_ignore_ascii_case("authorization") {
            bail!(
                "Refusing to override the Authorization header. \
                 Credentials come from the profile; see `atlassian-cli auth`."
            );
        }

        let name: HeaderName = name
            .parse()
            .with_context(|| format!("Invalid header name in '{entry}'"))?;
        let value: HeaderValue = value
            .parse()
            .with_context(|| format!("Invalid header value in '{entry}'"))?;
        headers.append(name, value);
    }
    Ok(headers)
}

/// Append `--query` pairs, percent-encoding them. Without this, a JQL query has
/// to be hand-encoded (`project%20%3D%20TEST`).
fn append_queries(path: &str, queries: &[String]) -> Result<String> {
    if queries.is_empty() {
        return Ok(path.to_string());
    }

    let mut out = String::from(path);
    for (i, entry) in queries.iter().enumerate() {
        let (key, value) = entry
            .split_once('=')
            .with_context(|| format!("Invalid --query '{entry}': expected key=value"))?;
        let separator = if i == 0 && !path.contains('?') {
            '?'
        } else {
            '&'
        };
        out.push(separator);
        out.push_str(&urlencoding::encode(key));
        out.push('=');
        out.push_str(&urlencoding::encode(value));
    }
    Ok(out)
}

#[derive(Debug, PartialEq)]
pub(crate) enum Rendered {
    Text(String),
    Binary,
    Empty,
}

/// Decide how to present a body. JSON is pretty-printed; a top-level array stays
/// JSON rather than becoming a table, which would drop nested fields.
pub(crate) fn format_body(bytes: &[u8], format: OutputFormat) -> Result<Rendered> {
    if bytes.is_empty() {
        return Ok(Rendered::Empty);
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Ok(Rendered::Binary),
    };
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) if format == OutputFormat::Yaml => Ok(Rendered::Text(
            serde_yaml::to_string(&value).context("Failed to convert response to YAML")?,
        )),
        Ok(value) => Ok(Rendered::Text(
            serde_json::to_string_pretty(&value).context("Failed to format response as JSON")?,
        )),
        // Not JSON (an HTML error page, plain text): pass it through untouched.
        Err(_) => Ok(Rendered::Text(text.to_string())),
    }
}

fn emit(renderer: &OutputRenderer, args: &ApiArgs, response: &RawResponse) -> Result<()> {
    if args.include {
        eprintln!("HTTP {}", response.status);
        for (name, value) in &response.headers {
            // Jira sets atlassian.xsrf.token here and this output gets pasted
            // into bug reports.
            let value = if name.eq_ignore_ascii_case("set-cookie") {
                "[REDACTED]"
            } else {
                value.as_str()
            };
            eprintln!("{name}: {value}");
        }
        eprintln!();
    }

    if let Some(path) = &args.output {
        std::fs::write(path, &response.body)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        return Ok(());
    }

    match format_body(&response.body, renderer.format())? {
        Rendered::Empty => Ok(()),
        Rendered::Text(text) => renderer.render_raw(&text),
        Rendered::Binary => {
            if std::io::stdout().is_terminal() {
                bail!(
                    "Response is binary ({} bytes, content-type {}). \
                     Use --output <FILE> or redirect stdout.",
                    response.body.len(),
                    response.header("content-type").unwrap_or("unknown")
                );
            }
            let mut out = std::io::stdout().lock();
            out.write_all(&response.body)
                .context("Failed to write response to stdout")?;
            out.flush().context("Failed to flush stdout")
        }
    }
}

pub async fn run(client: &ApiClient, renderer: &OutputRenderer, args: ApiArgs) -> Result<()> {
    let request = resolve(&args, &mut std::io::stdin())?;

    if args.dry_run {
        // Auth is applied inside the client at send time, so nothing secret can
        // appear here.
        println!("{} {}", request.method, client.resolve_url(&request.path)?);
        for (name, value) in request.headers.iter() {
            println!("{name}: {}", value.to_str().unwrap_or(""));
        }
        if let Some(body) = &request.body {
            println!("\n{}", String::from_utf8_lossy(body));
        }
        return Ok(());
    }

    // DELETE is the one verb whose accidental invocation cannot be undone.
    // Requiring --force for every write would be wrong: several Jira read
    // endpoints are POST. This exits non-zero rather than returning Ok like
    // `jira issue delete` does, because a passthrough that silently no-ops in a
    // script is worse than one that fails.
    if request.method == Method::DELETE && !args.force {
        bail!("Refusing to send DELETE without --force. Preview it with --dry-run.");
    }

    let response = client
        .request_raw(RawRequest {
            method: request.method.clone(),
            path: &request.path,
            headers: request.headers,
            body: request.body.as_deref(),
            timeout: args.timeout.map(Duration::from_secs),
        })
        .await
        .with_context(|| format!("Request failed: {} {}", request.method, request.path))?;

    let status = response.status;
    // The body is emitted first even on failure: it is the API's own answer, and
    // surfacing it is the whole point of a passthrough.
    emit(renderer, &args, &response)?;

    if !response.is_success() {
        bail!("HTTP {status} {} {}", request.method, request.path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn args(path: &str) -> ApiArgs {
        ApiArgs {
            path: path.to_string(),
            method: None,
            data: None,
            headers: Vec::new(),
            queries: Vec::new(),
            include: false,
            output: None,
            dry_run: false,
            force: false,
            timeout: None,
        }
    }

    #[test]
    fn default_method_is_get_without_a_body() {
        assert_eq!(default_method(None, false), Method::GET);
    }

    // Defaulting to GET with a body would silently drop the payload.
    #[test]
    fn default_method_is_post_with_a_body() {
        assert_eq!(default_method(None, true), Method::POST);
    }

    #[test]
    fn explicit_method_wins_over_the_body_default() {
        assert_eq!(default_method(Some(HttpMethod::Get), true), Method::GET);
    }

    #[test]
    fn append_queries_returns_the_path_unchanged_when_empty() {
        assert_eq!(
            append_queries("/rest/api/3/myself", &[]).unwrap(),
            "/rest/api/3/myself"
        );
    }

    #[test]
    fn append_queries_uses_question_mark_then_ampersand() {
        let q = vec!["a=1".to_string(), "b=2".to_string()];
        assert_eq!(append_queries("/x", &q).unwrap(), "/x?a=1&b=2");
    }

    #[test]
    fn append_queries_appends_to_an_existing_query_string() {
        let q = vec!["b=2".to_string()];
        assert_eq!(append_queries("/x?a=1", &q).unwrap(), "/x?a=1&b=2");
    }

    // The reason --query exists: JQL is full of spaces and equals signs.
    #[test]
    fn append_queries_encodes_spaces_and_equals() {
        let q = vec!["jql=project = TEST".to_string()];
        assert_eq!(
            append_queries("/search", &q).unwrap(),
            "/search?jql=project%20%3D%20TEST"
        );
    }

    #[test]
    fn append_queries_splits_on_the_first_equals_only() {
        let q = vec!["jql=a=b".to_string()];
        assert_eq!(append_queries("/x", &q).unwrap(), "/x?jql=a%3Db");
    }

    #[test]
    fn append_queries_rejects_a_pair_without_equals() {
        let q = vec!["oops".to_string()];
        let err = append_queries("/x", &q).unwrap_err();
        assert!(err.to_string().contains("oops"), "got {err}");
    }

    #[test]
    fn parse_headers_accepts_and_trims() {
        let headers = parse_headers(&["X-Test:  value  ".to_string()]).unwrap();
        assert_eq!(headers.get("x-test").unwrap(), "value");
    }

    #[test]
    fn parse_headers_keeps_duplicates() {
        let headers = parse_headers(&["X-A: 1".to_string(), "X-A: 2".to_string()]).unwrap();
        assert_eq!(headers.get_all("x-a").iter().count(), 2);
    }

    #[test]
    fn parse_headers_rejects_a_missing_colon() {
        assert!(parse_headers(&["nope".to_string()]).is_err());
    }

    // Shadowing the profile credential would be confusing and is never needed.
    #[test]
    fn parse_headers_rejects_authorization() {
        let err = parse_headers(&["authorization: Bearer x".to_string()]).unwrap_err();
        assert!(err.to_string().contains("auth"), "got {err}");
    }

    #[test]
    fn parse_headers_rejects_crlf_injection() {
        assert!(parse_headers(&["X-A: a\r\nX-B: b".to_string()]).is_err());
    }

    #[test]
    fn read_body_passes_inline_json_through_verbatim() {
        let mut stdin = Cursor::new(Vec::new());
        // Key order and spacing must survive; no reserialization.
        let raw = r#"{"b":1,"a":2}"#;
        assert_eq!(read_body(raw, &mut stdin).unwrap(), raw.as_bytes());
    }

    #[test]
    fn read_body_reads_stdin_for_dash() {
        let mut stdin = Cursor::new(b"from-stdin".to_vec());
        assert_eq!(read_body("-", &mut stdin).unwrap(), b"from-stdin");
    }

    #[test]
    fn read_body_reads_an_at_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.json");
        std::fs::write(&path, br#"{"x":1}"#).unwrap();
        let mut stdin = Cursor::new(Vec::new());
        let spec = format!("@{}", path.display());
        assert_eq!(read_body(&spec, &mut stdin).unwrap(), br#"{"x":1}"#);
    }

    #[test]
    fn read_body_reports_a_missing_file_by_path() {
        let mut stdin = Cursor::new(Vec::new());
        let err = read_body("@/nope/missing.json", &mut stdin).unwrap_err();
        assert!(err.to_string().contains("/nope/missing.json"), "got {err}");
    }

    #[test]
    fn resolve_sets_json_content_type_only_with_a_body() {
        let mut stdin = Cursor::new(Vec::new());
        let mut a = args("/x");
        assert!(resolve(&a, &mut stdin)
            .unwrap()
            .headers
            .get("content-type")
            .is_none());

        a.data = Some("{}".to_string());
        let resolved = resolve(&a, &mut stdin).unwrap();
        assert_eq!(
            resolved.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert_eq!(resolved.method, Method::POST);
    }

    #[test]
    fn resolve_lets_an_explicit_content_type_win() {
        let mut stdin = Cursor::new(Vec::new());
        let mut a = args("/x");
        a.data = Some("plain".to_string());
        a.headers = vec!["Content-Type: text/plain".to_string()];
        let resolved = resolve(&a, &mut stdin).unwrap();
        assert_eq!(resolved.headers.get("content-type").unwrap(), "text/plain");
    }

    #[test]
    fn format_body_pretty_prints_json() {
        let out = format_body(br#"{"a":1}"#, OutputFormat::Table).unwrap();
        assert_eq!(out, Rendered::Text("{\n  \"a\": 1\n}".to_string()));
    }

    /// Regression guard for the output contract: an array of objects would be a
    /// perfectly good table, and rendering it as one would drop nested fields.
    #[test]
    fn format_body_keeps_a_json_array_as_json() {
        let body = br#"[{"a":{"nested":1}},{"a":{"nested":2}}]"#;
        let out = format_body(body, OutputFormat::Table).unwrap();
        match out {
            Rendered::Text(text) => {
                assert!(text.starts_with('['), "got {text}");
                assert!(
                    text.contains("nested"),
                    "nested fields must survive: {text}"
                );
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn format_body_converts_json_to_yaml_when_asked() {
        let out = format_body(br#"{"a":1}"#, OutputFormat::Yaml).unwrap();
        assert_eq!(out, Rendered::Text("a: 1\n".to_string()));
    }

    #[test]
    fn format_body_passes_non_json_text_through() {
        let out = format_body(b"<html>oops</html>", OutputFormat::Json).unwrap();
        assert_eq!(out, Rendered::Text("<html>oops</html>".to_string()));
    }

    #[test]
    fn format_body_detects_binary() {
        assert_eq!(
            format_body(&[0xff, 0xfe, 0x00], OutputFormat::Json).unwrap(),
            Rendered::Binary
        );
    }

    // `request` coerces an empty 2xx body to JSON null; the raw path must not.
    #[test]
    fn format_body_treats_an_empty_body_as_empty() {
        assert_eq!(
            format_body(b"", OutputFormat::Json).unwrap(),
            Rendered::Empty
        );
    }
}
