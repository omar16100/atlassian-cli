use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

use super::utils::ConfluenceContext;

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
    let http_client = reqwest::Client::new();

    let mut request = http_client
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
    println!("✅ Uploaded attachment '{}' to page {}", file_name, page_id);
    Ok(())
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
    let http_client = reqwest::Client::new();

    let mut request = http_client.get(format!("{}{}", base_url, attachment.download_link));

    // Apply authentication
    request = ctx.client.apply_auth(request);

    let response = request
        .send()
        .await
        .context("Failed to download attachment")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to download attachment"));
    }

    let content = response.bytes().await.context("Failed to read attachment content")?;

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
    println!("✅ Deleted attachment: {}", attachment_id);
    Ok(())
}
