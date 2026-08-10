//! Jira attachment operations: list, get, download (single and bulk), upload, delete.
//!
//! Download goes through `ApiClient::get_bytes` against
//! `/rest/api/3/attachment/content/{id}`. That endpoint answers with a 302 to a
//! short-lived signed URL on Atlassian's media host; reqwest follows it and
//! strips the `Authorization` header on the cross-host hop, which is what we
//! want since the redirect target carries its own token. Going through
//! `ApiClient` also keeps the same-origin check, retries and rate limiting.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::utils::JiraContext;
use crate::commands::common::{render_success, MutationResult};

// ---------------------------------------------------------------------------
// Model (shared with `jira issue get`)
// ---------------------------------------------------------------------------

/// One Jira attachment, as returned both by `/rest/api/3/attachment/{id}` and
/// inside an issue's `attachment` field.
///
/// Jira returns `id` as a number in some responses and a string in others;
/// `de_id_to_string` normalizes both. Every field is optional so a single
/// missing or null property never aborts the parse of a whole issue.
#[derive(Deserialize, Serialize)]
pub(super) struct JiraAttachment {
    #[serde(
        default,
        deserialize_with = "de_id_to_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<AttachmentAuthor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct AttachmentAuthor {
    #[serde(
        rename = "displayName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
}

/// Deserialize a value that may be a JSON string, number, or null into an
/// `Option<String>` (used for Jira's number-or-string attachment `id`).
fn de_id_to_string<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|v| match v {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }))
}

/// Render an attachments section for markdown `issue get`. Empty string when the
/// issue has no attachments.
pub(super) fn attachments_markdown(attachments: &[JiraAttachment]) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n\n## Attachments\n\n| Filename | Size | ID | URL |\n| --- | --- | --- | --- |\n",
    );
    for a in attachments {
        let filename = a.filename.as_deref().unwrap_or("");
        let size = a.size.map(|s| s.to_string()).unwrap_or_default();
        let id = a.id.as_deref().unwrap_or("");
        let url = a.content.as_deref().unwrap_or("");
        out.push_str(&format!("| {filename} | {size} | {id} | {url} |\n"));
    }
    out
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Characters Windows forbids in a filename. Stripped even on Unix, because the
/// downloaded file may later be synced or copied to a Windows machine.
const ILLEGAL_CHARS: [char; 7] = ['<', '>', ':', '"', '|', '?', '*'];

/// Reserved DOS device names. A file named `CON` is unopenable on Windows.
const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Most filesystems cap a single path component at 255 bytes.
const MAX_FILENAME_BYTES: usize = 255;

/// Split a filename into (stem, extension), where the extension keeps its dot.
/// A leading dot is treated as part of the stem, so `.gitignore` has no extension.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

fn fallback_name(fallback_id: &str) -> String {
    let id: String = fallback_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if id.is_empty() {
        "attachment".to_string()
    } else {
        format!("attachment-{id}")
    }
}

fn guard_reserved(name: &str) -> String {
    let (stem, ext) = split_ext(name);
    if RESERVED_STEMS.iter().any(|r| stem.eq_ignore_ascii_case(r)) {
        format!("{stem}_{ext}")
    } else {
        name.to_string()
    }
}

fn truncate_filename(name: &str) -> String {
    if name.len() <= MAX_FILENAME_BYTES {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    // An absurdly long "extension" is not worth preserving.
    let ext = if ext.len() < MAX_FILENAME_BYTES / 2 {
        ext
    } else {
        ""
    };
    let mut end = (MAX_FILENAME_BYTES - ext.len()).min(stem.len());
    while end > 0 && !stem.is_char_boundary(end) {
        end -= 1;
    }
    let out = format!("{}{}", &stem[..end], ext);
    if out.is_empty() || out == ext {
        "attachment".to_string()
    } else {
        out
    }
}

/// Reduce a server-supplied attachment filename to exactly ONE safe path segment.
///
/// This is the single choke point that makes `dir.join(safe_filename(..))`
/// incapable of escaping `dir`: the result provably contains no `/` and no `\`,
/// and is never empty, `.` or `..`. Everything the server sends is untrusted, and
/// a compromised or malicious instance could answer with `../../.ssh/authorized_keys`.
fn safe_filename(raw: &str, fallback_id: &str) -> String {
    // Splitting on both separators collapses `../../etc/passwd`, `/etc/passwd`
    // and `..\..\Windows\evil.dll` to their basename. Backslash counts even on
    // Unix because the server, not the local OS, chose the string.
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !ILLEGAL_CHARS.contains(c))
        .collect();
    // Windows silently drops trailing dots and spaces, which would otherwise let
    // `evil.exe. ` land on disk as `evil.exe`.
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']).trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return fallback_name(fallback_id);
    }
    truncate_filename(&guard_reserved(trimmed))
}

/// Join a sanitized server filename onto a caller-supplied directory. The result
/// is always a direct child of `dir`.
fn resolve_download_path(dir: &Path, raw_filename: &str, fallback_id: &str) -> PathBuf {
    dir.join(safe_filename(raw_filename, fallback_id))
}

/// What `--output` asked for, decided without touching the filesystem.
#[derive(Debug, PartialEq)]
enum TargetSpec {
    /// `--output -`: stream bytes to stdout and print nothing else.
    Stdout,
    /// `--output <PATH>`: used verbatim, the user's own path is trusted.
    Explicit(PathBuf),
    /// No `--output`: the sanitized server filename, in the current directory.
    ServerNamedInCwd,
}

fn resolve_single_target(output: Option<&Path>) -> TargetSpec {
    match output {
        None => TargetSpec::ServerNamedInCwd,
        Some(p) if p == Path::new("-") => TargetSpec::Stdout,
        Some(p) => TargetSpec::Explicit(p.to_path_buf()),
    }
}

/// Jira allows two attachments on one issue to share a filename. Without this,
/// the second silently clobbers the first during a bulk download.
fn unique_download_name(taken: &HashSet<String>, filename: &str, id: &str) -> String {
    if !taken.contains(filename) {
        return filename.to_string();
    }
    let prefixed = format!("{id}-{filename}");
    if !taken.contains(&prefixed) {
        return prefixed;
    }
    let (stem, ext) = split_ext(&prefixed);
    let mut n = 2usize;
    loop {
        let candidate = format!("{stem}-{n}{ext}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Reject issue keys and attachment ids that could alter the request we build.
///
/// The multipart upload path formats its URL by hand and so never passes through
/// `ApiClient`'s same-origin check. The host cannot change (the value sits
/// mid-path), but `/`, `..`, `?`, `#` and percent escapes would change which
/// path or query the request actually hits.
fn validate_ref(kind: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{kind} must not be empty");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("Invalid {kind} '{value}': expected letters, digits, '-' or '_' only");
    }
    Ok(())
}

/// Basename of a local upload path, for the multipart `filename=` parameter.
fn upload_part_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("attachment")
        .to_string()
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AttachmentRow<'a> {
    id: &'a str,
    filename: &'a str,
    size: u64,
    mime_type: &'a str,
    author: &'a str,
    created: &'a str,
}

fn attachment_row(a: &JiraAttachment) -> AttachmentRow<'_> {
    AttachmentRow {
        id: a.id.as_deref().unwrap_or(""),
        filename: a.filename.as_deref().unwrap_or(""),
        size: a.size.unwrap_or(0),
        mime_type: a.mime_type.as_deref().unwrap_or(""),
        author: a
            .author
            .as_ref()
            .and_then(|u| u.display_name.as_deref())
            .unwrap_or(""),
        created: a.created.as_deref().unwrap_or(""),
    }
}

#[derive(Serialize)]
struct DownloadRow {
    id: String,
    filename: String,
    size: u64,
    path: String,
    status: String,
}

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

async fn fetch_issue_attachments(
    ctx: &JiraContext<'_>,
    issue_key: &str,
) -> Result<Vec<JiraAttachment>> {
    #[derive(Deserialize)]
    struct IssueAttachments {
        #[serde(default)]
        fields: AttachmentFields,
    }

    #[derive(Deserialize, Default)]
    struct AttachmentFields {
        #[serde(default)]
        attachment: Vec<JiraAttachment>,
    }

    let response: IssueAttachments = ctx
        .client
        .get(&format!("/rest/api/3/issue/{issue_key}?fields=attachment"))
        .await
        .with_context(|| format!("Failed to list attachments for issue {issue_key}"))?;

    Ok(response.fields.attachment)
}

async fn fetch_attachment_meta(
    ctx: &JiraContext<'_>,
    attachment_id: &str,
) -> Result<JiraAttachment> {
    ctx.client
        .get(&format!("/rest/api/3/attachment/{attachment_id}"))
        .await
        .with_context(|| format!("Failed to get attachment {attachment_id}"))
}

async fn fetch_attachment_bytes(ctx: &JiraContext<'_>, attachment_id: &str) -> Result<Vec<u8>> {
    ctx.client
        .get_bytes(&format!("/rest/api/3/attachment/content/{attachment_id}"))
        .await
        .with_context(|| format!("Failed to download attachment {attachment_id}"))
}

/// Write bytes to `path`, refusing to clobber an existing file without `force`.
fn write_bytes(path: &Path, bytes: &[u8], force: bool) -> Result<()> {
    if !force && path.exists() {
        bail!(
            "{} already exists. Use --force to overwrite.",
            path.display()
        );
    }
    fs::write(path, bytes).with_context(|| format!("Failed to write file: {}", path.display()))
}

fn write_stdout(bytes: &[u8]) -> Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(bytes)
        .context("Failed to write attachment to stdout")?;
    out.flush().context("Failed to flush stdout")
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// List the attachments on an issue.
pub async fn list_attachments(ctx: &JiraContext<'_>, issue_key: &str) -> Result<()> {
    validate_ref("issue key", issue_key)?;

    let attachments = fetch_issue_attachments(ctx, issue_key).await?;
    let rows: Vec<AttachmentRow<'_>> = attachments.iter().map(attachment_row).collect();

    ctx.renderer.render_list(&rows)
}

/// Show metadata for a single attachment.
pub async fn get_attachment(ctx: &JiraContext<'_>, attachment_id: &str) -> Result<()> {
    validate_ref("attachment id", attachment_id)?;

    let attachment = fetch_attachment_meta(ctx, attachment_id).await?;
    ctx.renderer.render(&attachment_row(&attachment))
}

/// Download one attachment's content.
pub async fn download_attachment(
    ctx: &JiraContext<'_>,
    attachment_id: &str,
    output: Option<&Path>,
    force: bool,
) -> Result<()> {
    validate_ref("attachment id", attachment_id)?;

    match resolve_single_target(output) {
        // Nothing but the bytes may reach stdout here: the stream has to stay
        // byte-exact for pipes. Logs go to stderr (see main::init_tracing).
        TargetSpec::Stdout => {
            let bytes = fetch_attachment_bytes(ctx, attachment_id).await?;
            write_stdout(&bytes)
        }
        // An explicit path needs no metadata call, so this is a single request.
        TargetSpec::Explicit(path) => {
            let bytes = fetch_attachment_bytes(ctx, attachment_id).await?;
            write_bytes(&path, &bytes, force)?;
            tracing::info!(%attachment_id, file = %path.display(), "Attachment downloaded");
            let message = format!(
                "Downloaded attachment {attachment_id} to {}",
                path.display()
            );
            render_success(
                ctx.renderer,
                &format!("✅ {message}"),
                &MutationResult::with_id(message.clone(), attachment_id),
            )
        }
        // Without --output the metadata call supplies the filename.
        TargetSpec::ServerNamedInCwd => {
            let meta = fetch_attachment_meta(ctx, attachment_id).await?;
            let raw = meta.filename.as_deref().unwrap_or("");
            let path = resolve_download_path(Path::new("."), raw, attachment_id);
            let bytes = fetch_attachment_bytes(ctx, attachment_id).await?;
            write_bytes(&path, &bytes, force)?;
            tracing::info!(%attachment_id, file = %path.display(), "Attachment downloaded");
            let message = format!("Downloaded attachment '{raw}' to {}", path.display());
            render_success(
                ctx.renderer,
                &format!("✅ {message}"),
                &MutationResult::with_id(message.clone(), attachment_id),
            )
        }
    }
}

/// Download every attachment on an issue into a directory.
///
/// Sequential on purpose: concurrent fetches would sidestep the shared rate
/// limiter. Partial failures are reported per file and produce a non-zero exit.
pub async fn download_issue_attachments(
    ctx: &JiraContext<'_>,
    issue_key: &str,
    dir: Option<&Path>,
    force: bool,
) -> Result<()> {
    validate_ref("issue key", issue_key)?;

    let dir = dir.unwrap_or(Path::new("."));
    fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create directory: {}", dir.display()))?;

    let attachments = fetch_issue_attachments(ctx, issue_key).await?;
    let total = attachments.len();
    let mut taken: HashSet<String> = HashSet::new();
    let mut rows: Vec<DownloadRow> = Vec::with_capacity(total);
    let mut failures = 0usize;

    for attachment in &attachments {
        let id = attachment.id.as_deref().unwrap_or("");
        let raw = attachment.filename.as_deref().unwrap_or("");
        let name = unique_download_name(&taken, &safe_filename(raw, id), id);
        taken.insert(name.clone());
        let path = dir.join(&name);

        let outcome = match fetch_attachment_bytes(ctx, id).await {
            Ok(bytes) => write_bytes(&path, &bytes, force).map(|()| bytes.len() as u64),
            Err(err) => Err(err),
        };

        match outcome {
            Ok(size) => rows.push(DownloadRow {
                id: id.to_string(),
                filename: name,
                size,
                path: path.display().to_string(),
                status: "ok".to_string(),
            }),
            Err(err) => {
                failures += 1;
                tracing::warn!(attachment_id = %id, error = %err, "Attachment download failed");
                rows.push(DownloadRow {
                    id: id.to_string(),
                    filename: name,
                    size: 0,
                    path: path.display().to_string(),
                    status: format!("failed: {err}"),
                });
            }
        }
    }

    ctx.renderer.render_list(&rows)?;

    if failures > 0 {
        bail!("{failures} of {total} attachment(s) failed to download");
    }
    Ok(())
}

/// Upload one or more files to an issue.
///
/// Multipart uploads go through the raw reqwest client, so unlike every other
/// command here they get no same-origin check, no retry and no rate limiting.
/// `validate_ref` on the issue key is the compensating control.
pub async fn upload_attachments(
    ctx: &JiraContext<'_>,
    issue_key: &str,
    files: &[PathBuf],
) -> Result<()> {
    validate_ref("issue key", issue_key)?;
    if files.is_empty() {
        bail!("At least one --file is required");
    }

    // The field name must be `file`, repeated once per upload.
    let mut form = reqwest::multipart::Form::new();
    for path in files {
        let content =
            fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))?;
        form = form.part(
            "file",
            reqwest::multipart::Part::bytes(content).file_name(upload_part_name(path)),
        );
    }

    // base_url() keeps its trailing slash, so trim before joining.
    let base_url = ctx.client.base_url().trim_end_matches('/').to_string();
    let mut request = ctx
        .client
        .http_client()
        .post(format!(
            "{base_url}/rest/api/3/issue/{issue_key}/attachments"
        ))
        .multipart(form)
        // Required by Jira to opt out of its XSRF check.
        .header("X-Atlassian-Token", "no-check");
    request = ctx.client.apply_auth(request);

    let response = request
        .send()
        .await
        .with_context(|| format!("Failed to upload attachments to issue {issue_key}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("Failed to upload attachments (HTTP {status}): {body}");
    }

    let uploaded: Vec<JiraAttachment> = response
        .json()
        .await
        .context("Failed to parse upload response")?;

    let names: Vec<&str> = uploaded
        .iter()
        .filter_map(|a| a.filename.as_deref())
        .collect();
    let ids: Vec<&str> = uploaded.iter().filter_map(|a| a.id.as_deref()).collect();

    tracing::info!(%issue_key, count = uploaded.len(), "Attachments uploaded");
    let message = format!(
        "Uploaded {} attachment(s) to {issue_key}: {}",
        uploaded.len(),
        names.join(", ")
    );
    render_success(
        ctx.renderer,
        &format!("✅ {message}"),
        &MutationResult::with_id(message.clone(), ids.join(",")),
    )
}

/// Delete an attachment.
pub async fn delete_attachment(
    ctx: &JiraContext<'_>,
    attachment_id: &str,
    force: bool,
) -> Result<()> {
    validate_ref("attachment id", attachment_id)?;

    if !force {
        println!(
            "⚠️  This will permanently delete attachment {attachment_id}. Use --force to confirm."
        );
        return Ok(());
    }

    ctx.client
        .delete_no_content(&format!("/rest/api/3/attachment/{attachment_id}"))
        .await
        .with_context(|| format!("Failed to delete attachment {attachment_id}"))?;

    tracing::info!(%attachment_id, "Attachment deleted successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Deleted attachment: {attachment_id}"),
        &MutationResult::with_id(
            format!("Deleted attachment: {attachment_id}"),
            attachment_id,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const FALLBACK: &str = "10001";

    #[test]
    fn safe_filename_passes_through_plain_name() {
        assert_eq!(safe_filename("report.pdf", FALLBACK), "report.pdf");
    }

    // The whole point of the helper: a hostile filename cannot escape the target
    // directory, because the result is a single path segment.
    #[test]
    fn safe_filename_strips_unix_traversal() {
        assert_eq!(safe_filename("../../etc/passwd", FALLBACK), "passwd");
    }

    #[test]
    fn safe_filename_strips_windows_traversal() {
        assert_eq!(
            safe_filename(r"..\..\Windows\System32\evil.dll", FALLBACK),
            "evil.dll"
        );
    }

    #[test]
    fn safe_filename_absolute_path_becomes_basename() {
        assert_eq!(safe_filename("/etc/shadow", FALLBACK), "shadow");
    }

    #[test]
    fn safe_filename_dot_dot_uses_fallback() {
        assert_eq!(safe_filename("..", FALLBACK), "attachment-10001");
    }

    #[test]
    fn safe_filename_single_dot_uses_fallback() {
        assert_eq!(safe_filename(".", FALLBACK), "attachment-10001");
    }

    #[test]
    fn safe_filename_empty_uses_fallback() {
        assert_eq!(safe_filename("", FALLBACK), "attachment-10001");
    }

    #[test]
    fn safe_filename_whitespace_only_uses_fallback() {
        assert_eq!(safe_filename("   ", FALLBACK), "attachment-10001");
    }

    // A trailing separator leaves an empty basename.
    #[test]
    fn safe_filename_trailing_separator_uses_fallback() {
        assert_eq!(safe_filename("evil/", FALLBACK), "attachment-10001");
    }

    #[test]
    fn safe_filename_empty_fallback_id_still_yields_a_name() {
        assert_eq!(safe_filename("", ""), "attachment");
    }

    #[test]
    fn safe_filename_strips_control_chars() {
        assert_eq!(safe_filename("a\0b\nc.txt", FALLBACK), "abc.txt");
    }

    #[test]
    fn safe_filename_strips_windows_illegal_chars() {
        assert_eq!(safe_filename("a:b|c?.txt", FALLBACK), "abc.txt");
    }

    // Windows silently drops trailing dots and spaces, so strip them ourselves.
    #[test]
    fn safe_filename_strips_trailing_dot_and_space() {
        assert_eq!(safe_filename("evil.exe. ", FALLBACK), "evil.exe");
    }

    #[test]
    fn safe_filename_windows_reserved_device_is_suffixed() {
        assert_eq!(safe_filename("CON", FALLBACK), "CON_");
        assert_eq!(safe_filename("nul.txt", FALLBACK), "nul_.txt");
    }

    #[test]
    fn safe_filename_preserves_unicode() {
        assert_eq!(
            safe_filename("naïve spec – v2.pdf", FALLBACK),
            "naïve spec – v2.pdf"
        );
    }

    #[test]
    fn safe_filename_truncates_overlong_name_on_char_boundary() {
        let long = format!("{}.pdf", "é".repeat(400));
        let out = safe_filename(&long, FALLBACK);
        assert!(out.len() <= MAX_FILENAME_BYTES, "len was {}", out.len());
        assert!(out.ends_with(".pdf"));
        // Truncating mid-codepoint would have panicked or produced invalid UTF-8.
        assert!(out.chars().all(|c| c == 'é' || ".pdf".contains(c)));
    }

    #[test]
    fn resolve_download_path_never_escapes_dir() {
        let dir = Path::new("/tmp/out");
        let hostile = [
            "../../etc/passwd",
            r"..\..\Windows\evil.dll",
            "/etc/shadow",
            "..",
            ".",
            "",
            "   ",
            "evil/",
            "a\0b.txt",
        ];
        for raw in hostile {
            let p = resolve_download_path(dir, raw, FALLBACK);
            assert!(p.starts_with(dir), "{raw} escaped to {}", p.display());
            assert_eq!(
                p.components().count(),
                dir.components().count() + 1,
                "{raw} produced extra components: {}",
                p.display()
            );
        }
    }

    proptest! {
        // Whatever the server sends, the result is one usable path segment.
        #[test]
        fn prop_safe_filename_is_a_single_segment(raw in ".*") {
            let out = safe_filename(&raw, FALLBACK);
            prop_assert!(!out.is_empty());
            prop_assert!(!out.contains('/'));
            prop_assert!(!out.contains('\\'));
            prop_assert!(out != "." && out != "..");
            prop_assert!(out.len() <= MAX_FILENAME_BYTES);
        }
    }

    #[test]
    fn single_target_dash_is_stdout() {
        assert_eq!(
            resolve_single_target(Some(Path::new("-"))),
            TargetSpec::Stdout
        );
    }

    // The user's own path is trusted, exactly like `curl -o`.
    #[test]
    fn single_target_explicit_path_used_verbatim() {
        assert_eq!(
            resolve_single_target(Some(Path::new("../out.bin"))),
            TargetSpec::Explicit(PathBuf::from("../out.bin"))
        );
    }

    #[test]
    fn single_target_none_is_server_named() {
        assert_eq!(resolve_single_target(None), TargetSpec::ServerNamedInCwd);
    }

    #[test]
    fn unique_name_passes_through_when_free() {
        let taken = HashSet::new();
        assert_eq!(unique_download_name(&taken, "dup.png", "10002"), "dup.png");
    }

    #[test]
    fn unique_name_prefixes_id_on_collision() {
        let taken: HashSet<String> = ["dup.png".to_string()].into_iter().collect();
        assert_eq!(
            unique_download_name(&taken, "dup.png", "10002"),
            "10002-dup.png"
        );
    }

    #[test]
    fn unique_name_appends_counter_when_id_prefix_also_taken() {
        let taken: HashSet<String> = ["dup.png".to_string(), "10002-dup.png".to_string()]
            .into_iter()
            .collect();
        assert_eq!(
            unique_download_name(&taken, "dup.png", "10002"),
            "10002-dup-2.png"
        );
    }

    #[test]
    fn validate_ref_accepts_keys_and_ids() {
        assert!(validate_ref("issue key", "PROJ-123").is_ok());
        assert!(validate_ref("attachment id", "10001").is_ok());
    }

    #[test]
    fn validate_ref_rejects_path_and_query_manipulation() {
        for bad in [
            "PROJ/../admin",
            "..",
            "x@evil.com",
            "a%2F",
            "",
            "a b",
            "a?b",
            "a#b",
        ] {
            assert!(
                validate_ref("issue key", bad).is_err(),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn upload_part_name_uses_basename() {
        assert_eq!(upload_part_name(Path::new("./docs/a.png")), "a.png");
    }

    #[test]
    fn upload_part_name_handles_dotfile() {
        assert_eq!(upload_part_name(Path::new("/tmp/.env")), ".env");
    }

    #[test]
    fn upload_part_name_empty_path_falls_back() {
        assert_eq!(upload_part_name(Path::new("")), "attachment");
    }

    #[test]
    fn de_id_to_string_accepts_number_string_and_null() {
        let numeric: JiraAttachment = serde_json::from_str(r#"{"id": 10001}"#).unwrap();
        assert_eq!(numeric.id.as_deref(), Some("10001"));
        let string: JiraAttachment = serde_json::from_str(r#"{"id": "10001"}"#).unwrap();
        assert_eq!(string.id.as_deref(), Some("10001"));
        let null: JiraAttachment = serde_json::from_str(r#"{"id": null}"#).unwrap();
        assert!(null.id.is_none());
    }

    #[test]
    fn attachment_deserializes_author_and_created() {
        let a: JiraAttachment = serde_json::from_str(
            r#"{"id":"1","filename":"a.png","author":{"displayName":"Ada"},"created":"2026-08-01T10:00:00.000+0000"}"#,
        )
        .unwrap();
        let row = attachment_row(&a);
        assert_eq!(row.author, "Ada");
        assert_eq!(row.created, "2026-08-01T10:00:00.000+0000");
    }

    #[test]
    fn attachment_row_tolerates_missing_fields() {
        let a: JiraAttachment = serde_json::from_str("{}").unwrap();
        let row = attachment_row(&a);
        assert_eq!(row.id, "");
        assert_eq!(row.size, 0);
        assert_eq!(row.author, "");
    }
}
