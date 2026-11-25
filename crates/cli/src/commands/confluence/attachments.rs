use anyhow::{Context, Result};
use atlassian_cli_api::path::encode_path_segment;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use super::utils::{fetch_v2_list, ConfluenceContext};
use crate::commands::common::{render_success, write_bytes, MutationResult};
use crate::query::UrlParamsBuilder;

// List attachments
pub async fn list_attachments(ctx: &ConfluenceContext<'_>, page_id: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct AttachmentsResponse {
        results: Vec<Attachment>,
    }

    #[derive(Deserialize)]
    struct Attachment {
        id: String,
        title: String,
        #[serde(rename = "fileSize")]
        file_size: i64,
        #[serde(rename = "mediaType")]
        media_type: String,
    }

    let response: AttachmentsResponse = ctx
        .client
        .get(&format!("/wiki/api/v2/pages/{}/attachments", page_id))
        .await
        .with_context(|| format!("Failed to list attachments for page {}", page_id))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        title: &'a str,
        file_size: i64,
        media_type: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .results
        .iter()
        .map(|a| Row {
            id: a.id.as_str(),
            title: a.title.as_str(),
            file_size: a.file_size,
            media_type: a.media_type.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Get attachment details
pub async fn get_attachment(ctx: &ConfluenceContext<'_>, attachment_id: &str) -> Result<()> {
    let attachment: Value = ctx
        .client
        .get(&format!("/wiki/api/v2/attachments/{}", attachment_id))
        .await
        .with_context(|| format!("Failed to get attachment {}", attachment_id))?;

    println!("{}", serde_json::to_string_pretty(&attachment)?);
    Ok(())
}

// Upload attachment
pub async fn upload_attachment(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    file_path: &PathBuf,
    comment: Option<&str>,
) -> Result<()> {
    let file_content = fs::read(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment");

    // Create multipart form data
    let form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(file_content).file_name(file_name.to_string()),
        )
        .text("minorEdit", "true");

    let form = if let Some(c) = comment {
        form.text("comment", c.to_string())
    } else {
        form
    };

    // Note: This uses the raw reqwest client for multipart upload, so the
    // central path guard does not run. The page id is encoded here instead.
    // base_url() keeps its trailing slash, so trim before joining.
    let base_url = ctx.client.base_url().trim_end_matches('/');

    let mut request = ctx
        .client
        .http_client()
        .post(format!(
            "{}/wiki/rest/api/content/{}/child/attachment",
            base_url,
            encode_path_segment(page_id)?
        ))
        .multipart(form)
        .header("X-Atlassian-Token", "no-check");

    // Apply authentication
    request = ctx.client.apply_auth(request);

    let response = request
        .send()
        .await
        .with_context(|| format!("Failed to upload attachment to page {}", page_id))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Failed to upload attachment: {}",
            error_text
        ));
    }

    tracing::info!(%page_id, file = %file_name, "Attachment uploaded successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Uploaded attachment '{file_name}' to page {page_id}"),
        &MutationResult::with_id(
            format!("Uploaded attachment '{file_name}' to page {page_id}"),
            page_id,
        ),
    )
}

/// Resolve an attachment's `downloadLink` into a path `ApiClient` can fetch.
///
/// The Confluence Cloud v2 API returns `downloadLink` relative to the `/wiki`
/// context path (e.g. `/download/attachments/{id}/{file}?version=1&api=v2`),
/// while `ApiClient` is rooted at the bare site origin and every Confluence
/// command spells `/wiki` itself. Concatenating base_url + downloadLink
/// therefore dropped the `/wiki` segment and every download 404'd.
///
/// Idempotent: a link that already carries `/wiki`, or an absolute URL, is
/// returned unchanged.
fn attachment_download_path(download_link: &str) -> String {
    if download_link.starts_with("http://")
        || download_link.starts_with("https://")
        || download_link == "/wiki"
        || download_link.starts_with("/wiki/")
    {
        return download_link.to_string();
    }
    format!("/wiki/{}", download_link.trim_start_matches('/'))
}

/// The fields of a v2 attachment a download needs. The single-attachment
/// endpoint and the page listing both return them.
#[derive(Deserialize)]
struct AttachmentDetail {
    id: String,
    #[serde(rename = "downloadLink")]
    download_link: String,
    title: String,
}

/// Turn a server-supplied attachment title into a file name that stays inside
/// the target directory.
///
/// The title is chosen by whoever uploaded the file, so it is not trusted: only
/// the last component after `/` or `\` is kept, control characters are
/// dropped, and an empty result, `.` or `..` falls back to `attachment-<id>`.
fn safe_filename(title: &str, attachment_id: &str) -> String {
    let last = title.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars().filter(|c| !c.is_control()).collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        format!("attachment-{attachment_id}")
    } else {
        cleaned.to_string()
    }
}

/// Make `name` unique among the names already used in one `--page` run.
///
/// Two attachments of a page can share a title. The second one gets its id
/// before the extension (`diagram-att2.png`). The comparison ignores case,
/// because macOS and Windows file systems do.
fn unique_filename(taken: &mut HashSet<String>, name: &str, attachment_id: &str) -> String {
    let candidate = if taken.contains(&name.to_lowercase()) {
        match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => format!("{stem}-{attachment_id}.{ext}"),
            _ => format!("{name}-{attachment_id}"),
        }
    } else {
        name.to_string()
    };
    taken.insert(candidate.to_lowercase());
    candidate
}

/// Fetch the bytes of one attachment.
///
/// Goes through ApiClient rather than a raw request: it applies auth, retries,
/// rate limiting and the same-origin (SSRF) check, matching bamboo's
/// download_artifact. The query string on downloadLink is preserved.
async fn fetch_attachment_bytes(
    ctx: &ConfluenceContext<'_>,
    attachment: &AttachmentDetail,
) -> Result<Vec<u8>> {
    ctx.client
        .get_bytes(&attachment_download_path(&attachment.download_link))
        .await
        .with_context(|| format!("Failed to download attachment {}", attachment.id))
}

/// Download one attachment.
///
/// `output` is a file path, `-` for stdout, or `None` for the attachment title
/// in the current directory. With `-` the bytes are the only thing written to
/// stdout, so `> file` and `| file -` get a clean stream.
pub async fn download_attachment(
    ctx: &ConfluenceContext<'_>,
    attachment_id: &str,
    output: Option<&Path>,
    force: bool,
) -> Result<()> {
    let attachment: AttachmentDetail = ctx
        .client
        .get(&format!(
            "/wiki/api/v2/attachments/{}",
            encode_path_segment(attachment_id)?
        ))
        .await
        .with_context(|| format!("Failed to get attachment {}", attachment_id))?;

    if output == Some(Path::new("-")) {
        if std::io::stdout().is_terminal() {
            anyhow::bail!(
                "Refusing to write attachment bytes to a terminal. \
                 Use --output <FILE> or redirect stdout."
            );
        }
        let content = fetch_attachment_bytes(ctx, &attachment).await?;
        let mut out = std::io::stdout().lock();
        out.write_all(&content)
            .context("Failed to write attachment to stdout")?;
        out.flush().context("Failed to flush stdout")?;
        tracing::info!(%attachment_id, "Attachment written to stdout");
        return Ok(());
    }

    let target: PathBuf = match output {
        Some(path) => path.to_path_buf(),
        None => PathBuf::from(safe_filename(&attachment.title, &attachment.id)),
    };

    let content = fetch_attachment_bytes(ctx, &attachment).await?;
    write_bytes(&target, &content, force)?;

    tracing::info!(attachment_id = %attachment_id, file = %target.display(), "Attachment downloaded successfully");
    println!(
        "✅ Downloaded attachment '{}' to {}",
        attachment.title,
        target.display()
    );
    Ok(())
}

/// Download every attachment of a page into `dir`.
///
/// All pages of the attachment listing are read. A failed file does not stop
/// the others; the command lists every failure on stderr and exits non-zero
/// at the end.
pub async fn download_page_attachments(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    dir: &Path,
    force: bool,
) -> Result<()> {
    let attachments: Vec<AttachmentDetail> = fetch_v2_list(
        ctx,
        &format!(
            "/wiki/api/v2/pages/{}/attachments",
            encode_path_segment(page_id)?
        ),
        UrlParamsBuilder::new(),
        Some(0),
        "attachments",
    )
    .await
    .with_context(|| format!("Failed to list attachments for page {}", page_id))?;

    if attachments.is_empty() {
        println!("Page {page_id} has no attachments.");
        return Ok(());
    }

    fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create directory: {}", dir.display()))?;

    let mut taken = HashSet::new();
    let mut failures = Vec::new();
    for attachment in &attachments {
        let name = unique_filename(
            &mut taken,
            &safe_filename(&attachment.title, &attachment.id),
            &attachment.id,
        );
        let target = dir.join(&name);
        let result = match fetch_attachment_bytes(ctx, attachment).await {
            Ok(content) => write_bytes(&target, &content, force),
            Err(err) => Err(err),
        };
        match result {
            Ok(()) => println!("✅ {} -> {}", attachment.id, target.display()),
            Err(err) => {
                eprintln!("❌ {}: {:#}", attachment.id, err);
                failures.push(attachment.id.clone());
            }
        }
    }

    let done = attachments.len() - failures.len();
    tracing::info!(%page_id, downloaded = done, failed = failures.len(), "Page attachments downloaded");
    if failures.is_empty() {
        println!(
            "Downloaded {done} attachment(s) of page {page_id} to {}",
            dir.display()
        );
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "{} of {} attachment(s) of page {} failed: {}",
            failures.len(),
            attachments.len(),
            page_id,
            failures.join(", ")
        ))
    }
}

// Delete attachment
pub async fn delete_attachment(
    ctx: &ConfluenceContext<'_>,
    attachment_id: &str,
    force: bool,
) -> Result<()> {
    if !force {
        println!(
            "⚠️  This will permanently delete attachment {}. Use --force to confirm.",
            attachment_id
        );
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!(
            "/wiki/api/v2/attachments/{}",
            encode_path_segment(attachment_id)?
        ))
        .await
        .with_context(|| format!("Failed to delete attachment {}", attachment_id))?;

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

    // Regression: the v2 API returns downloadLink relative to the /wiki context
    // (no /wiki prefix). ApiClient is rooted at the bare origin, so we insert /wiki.
    #[test]
    fn bare_relative_link_gets_wiki_prefix() {
        assert_eq!(
            attachment_download_path("/download/attachments/100/diagram.png"),
            "/wiki/download/attachments/100/diagram.png"
        );
    }

    // The real v2 downloadLink carries a query string; it must survive intact.
    #[test]
    fn query_string_is_preserved() {
        assert_eq!(
            attachment_download_path("/download/attachments/1/f.png?version=1&api=v2"),
            "/wiki/download/attachments/1/f.png?version=1&api=v2"
        );
    }

    // Idempotent: a link that already includes /wiki is left untouched.
    #[test]
    fn already_wiki_prefixed_link_is_unchanged() {
        let link = "/wiki/download/attachments/100/diagram.png";
        assert_eq!(attachment_download_path(link), link);
    }

    // Absolute links pass through. ApiClient::get_bytes then applies its
    // same-origin check, so a cross-host link is rejected rather than fetched.
    #[test]
    fn absolute_link_is_used_as_is() {
        let link = "https://example.atlassian.net/wiki/download/attachments/1/a.png";
        assert_eq!(attachment_download_path(link), link);
    }

    // Defensive: a link with no leading slash still produces a well-formed path.
    #[test]
    fn relative_link_without_leading_slash_is_handled() {
        assert_eq!(
            attachment_download_path("download/attachments/1/f.png"),
            "/wiki/download/attachments/1/f.png"
        );
    }

    // A plain title is used unchanged.
    #[test]
    fn plain_title_is_the_file_name() {
        assert_eq!(safe_filename("diagram.svg", "att1"), "diagram.svg");
    }

    // A title cannot point outside the target directory.
    #[test]
    fn path_components_in_the_title_are_dropped() {
        assert_eq!(safe_filename("../../etc/passwd", "att1"), "passwd");
        assert_eq!(safe_filename("..\\..\\boot.ini", "att1"), "boot.ini");
        assert_eq!(safe_filename("/abs/path/x.png", "att1"), "x.png");
    }

    // Titles that leave nothing usable fall back to the attachment id.
    #[test]
    fn unusable_titles_fall_back_to_the_id() {
        for title in ["", "  ", ".", "..", "dir/", "a/..", "\u{0007}"] {
            assert_eq!(
                safe_filename(title, "att9"),
                "attachment-att9",
                "title {title:?}"
            );
        }
    }

    // Control characters are removed, the rest of the name stays.
    #[test]
    fn control_characters_are_removed() {
        assert_eq!(safe_filename("re\nport\u{0000}.pdf", "att1"), "report.pdf");
    }

    // Two attachments with the same title get distinct names, case-insensitively.
    #[test]
    fn duplicate_titles_get_the_id_before_the_extension() {
        let mut taken = HashSet::new();
        assert_eq!(unique_filename(&mut taken, "a.png", "att1"), "a.png");
        assert_eq!(unique_filename(&mut taken, "A.png", "att2"), "A-att2.png");
        assert_eq!(unique_filename(&mut taken, "README", "att3"), "README");
        assert_eq!(unique_filename(&mut taken, "readme", "att4"), "readme-att4");
        assert_eq!(unique_filename(&mut taken, ".env", "att5"), ".env");
        assert_eq!(unique_filename(&mut taken, ".env", "att6"), ".env-att6");
    }

    // No double slash even if the API ever returned one.
    #[test]
    fn double_leading_slash_is_normalised() {
        assert_eq!(
            attachment_download_path("//download/attachments/1/f.png"),
            "/wiki/download/attachments/1/f.png"
        );
    }
}
