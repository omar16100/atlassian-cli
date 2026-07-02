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
    let base_url = ctx.client.base_url();

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

/// Build the absolute URL to fetch an attachment's bytes from its `downloadLink`.
///
/// The Confluence Cloud v2 API returns `downloadLink` as a path relative to the
/// `/wiki` context (e.g. `/rest/api/content/{id}/child/attachment/{att}/download`),
/// but `ApiClient::base_url()` is the bare site origin (no `/wiki`) — the command
/// paths add `/wiki` themselves. Naively concatenating base_url + downloadLink
/// therefore dropped the `/wiki` segment and every download failed. This joins
/// them correctly and is idempotent: already-absolute links and links that
/// already include the `/wiki` prefix are left untouched.
fn build_attachment_download_url(base_url: &str, download_link: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if download_link.starts_with("http://") || download_link.starts_with("https://") {
        download_link.to_string()
    } else if download_link.starts_with("/wiki/") || download_link == "/wiki" {
        format!("{base}{download_link}")
    } else {
        let sep = if download_link.starts_with('/') {
            ""
        } else {
            "/"
        };
        format!("{base}/wiki{sep}{download_link}")
    }
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

    // Download the file
    let base_url = ctx.client.base_url();
    let download_url = build_attachment_download_url(base_url, &attachment.download_link);

    let mut request = ctx.client.http_client().get(download_url);

    // Apply authentication
    request = ctx.client.apply_auth(request);

    let response = request
        .send()
        .await
        .context("Failed to download attachment")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to download attachment"));
    }

    let content = response
        .bytes()
        .await
        .context("Failed to read attachment content")?;

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

    const BASE: &str = "https://example.atlassian.net";

    // Regression: the v2 API returns downloadLink relative to the /wiki context
    // (no /wiki prefix). base_url is the bare origin, so we must insert /wiki.
    #[test]
    fn bare_relative_link_gets_wiki_prefix() {
        let link = "/rest/api/content/100/child/attachment/att1/download";
        assert_eq!(
            build_attachment_download_url(BASE, link),
            "https://example.atlassian.net/wiki/rest/api/content/100/child/attachment/att1/download"
        );
    }

    // Idempotent: a link that already includes /wiki is left untouched (this is
    // what the existing get_attachment test fixture returns).
    #[test]
    fn already_wiki_prefixed_link_is_unchanged() {
        let link = "/wiki/download/attachments/100/diagram.png";
        assert_eq!(
            build_attachment_download_url(BASE, link),
            "https://example.atlassian.net/wiki/download/attachments/100/diagram.png"
        );
    }

    #[test]
    fn absolute_link_is_used_as_is() {
        let link = "https://media.atlassian.com/file/abc/binary?token=xyz";
        assert_eq!(build_attachment_download_url(BASE, link), link);
    }

    #[test]
    fn trailing_slash_on_base_url_does_not_double_up() {
        let link = "/rest/api/content/1/child/attachment/a/download";
        assert_eq!(
            build_attachment_download_url("https://example.atlassian.net/", link),
            "https://example.atlassian.net/wiki/rest/api/content/1/child/attachment/a/download"
        );
    }

    // Defensive: a link with no leading slash still produces a well-formed URL.
    #[test]
    fn relative_link_without_leading_slash_is_handled() {
        assert_eq!(
            build_attachment_download_url(BASE, "download/attachments/1/f.png"),
            "https://example.atlassian.net/wiki/download/attachments/1/f.png"
        );
    }
}
