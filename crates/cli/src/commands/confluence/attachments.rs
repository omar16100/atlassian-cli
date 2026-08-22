use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

use super::utils::ConfluenceContext;
use crate::commands::common::{render_success, MutationResult};

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

    // Note: This uses the raw reqwest client for multipart upload
    // base_url() keeps its trailing slash, so trim before joining.
    let base_url = ctx.client.base_url().trim_end_matches('/');

    let mut request = ctx
        .client
        .http_client()
        .post(format!(
            "{}/wiki/rest/api/content/{}/child/attachment",
            base_url, page_id
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

// Download attachment
pub async fn download_attachment(
    ctx: &ConfluenceContext<'_>,
    attachment_id: &str,
    output: &PathBuf,
) -> Result<()> {
    // Get attachment details first to get download URL
    #[derive(Deserialize)]
    struct AttachmentDetail {
        #[serde(rename = "downloadLink")]
        download_link: String,
        title: String,
    }

    let attachment: AttachmentDetail = ctx
        .client
        .get(&format!("/wiki/api/v2/attachments/{}", attachment_id))
        .await
        .with_context(|| format!("Failed to get attachment {}", attachment_id))?;

    // Fetch through ApiClient rather than a raw request: it applies auth, retries,
    // rate limiting and the same-origin (SSRF) check, matching bamboo's
    // download_artifact. The query string on downloadLink is preserved.
    let content = ctx
        .client
        .get_bytes(&attachment_download_path(&attachment.download_link))
        .await
        .with_context(|| format!("Failed to download attachment {}", attachment_id))?;

    fs::write(output, content)
        .with_context(|| format!("Failed to write file: {}", output.display()))?;

    tracing::info!(attachment_id = %attachment_id, file = %output.display(), "Attachment downloaded successfully");
    println!(
        "✅ Downloaded attachment '{}' to {}",
        attachment.title,
        output.display()
    );
    Ok(())
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
        .delete(&format!("/wiki/api/v2/attachments/{}", attachment_id))
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

    // No double slash even if the API ever returned one.
    #[test]
    fn double_leading_slash_is_normalised() {
        assert_eq!(
            attachment_download_path("//download/attachments/1/f.png"),
            "/wiki/download/attachments/1/f.png"
        );
    }
}
