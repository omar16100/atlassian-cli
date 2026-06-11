use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use super::utils::{resolve_space_id, ConfluenceContext};
use crate::commands::common::{render_success, MutationResult};

/// The fields we read back from a folder create. The full response carries more
/// (position, authorId, createdAt, version, _links, ...) which we don't need here.
#[derive(Deserialize)]
struct CreatedFolder {
    id: String,
    #[serde(default)]
    title: Option<String>,
}

/// Build the POST body for `POST /wiki/api/v2/folders`. Pure and testable.
fn build_folder_payload(space_id: &str, title: &str, parent_id: Option<&str>) -> Value {
    let mut payload = json!({
        "spaceId": space_id,
        "title": title,
    });
    if let Some(pid) = parent_id {
        payload["parentId"] = json!(pid);
    }
    payload
}

/// `confluence folder get <FOLDER_ID>` -> GET /wiki/api/v2/folders/{id}
pub async fn get_folder(ctx: &ConfluenceContext<'_>, folder_id: &str) -> Result<()> {
    // Render the full response so no fields are dropped, matching `page get`.
    let folder: Value = ctx
        .client
        .get(&format!("/wiki/api/v2/folders/{folder_id}"))
        .await
        .with_context(|| format!("Failed to get folder {folder_id}"))?;

    ctx.renderer.render(&folder)
}

/// `confluence folder create --space <KEY> --title <TITLE> [--parent <ID>]`
/// -> POST /wiki/api/v2/folders
///
/// The v2 API expects a numeric `spaceId`, so the space key is resolved first.
/// `parentId` may reference either a page or another folder.
pub async fn create_folder(
    ctx: &ConfluenceContext<'_>,
    space_key: &str,
    title: &str,
    parent_id: Option<&str>,
) -> Result<()> {
    let space_id = resolve_space_id(ctx, space_key).await?;
    let payload = build_folder_payload(&space_id, title, parent_id);

    let folder: CreatedFolder = ctx
        .client
        .post("/wiki/api/v2/folders", &payload)
        .await
        .with_context(|| format!("Failed to create folder '{title}' in space {space_key}"))?;

    let created_title = folder.title.as_deref().unwrap_or(title);
    tracing::info!(id = %folder.id, title = %created_title, "Folder created successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Created folder: {created_title} (ID: {})", folder.id),
        &MutationResult::with_id(format!("Created folder: {created_title}"), &folder.id),
    )
}

/// `confluence folder delete <FOLDER_ID>` -> DELETE /wiki/api/v2/folders/{id}
///
/// Confluence moves the folder to the trash (it can be restored), rather than
/// erasing it immediately.
pub async fn delete_folder(
    ctx: &ConfluenceContext<'_>,
    folder_id: &str,
    force: bool,
) -> Result<()> {
    if !force {
        println!("⚠️  This will move folder {folder_id} to the trash. Use --force to confirm.");
        return Ok(());
    }

    ctx.client
        .delete_no_content(&format!("/wiki/api/v2/folders/{folder_id}"))
        .await
        .with_context(|| format!("Failed to delete folder {folder_id}"))?;

    tracing::info!(%folder_id, "Folder moved to trash");
    render_success(
        ctx.renderer,
        &format!("✅ Moved folder to trash: {folder_id}"),
        &MutationResult::with_id(format!("Moved folder to trash: {folder_id}"), folder_id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_without_parent_has_space_and_title_only() {
        let payload = build_folder_payload("3302031364", "Architecture", None);
        assert_eq!(payload["spaceId"], "3302031364");
        assert_eq!(payload["title"], "Architecture");
        assert!(payload.get("parentId").is_none());
    }

    #[test]
    fn payload_with_parent_includes_parent_id() {
        let payload = build_folder_payload("3302031364", "Sub", Some("3302031761"));
        assert_eq!(payload["parentId"], "3302031761");
    }

    #[test]
    fn created_folder_extracts_id_and_title_ignoring_extra_fields() {
        let json = r#"{
            "id": "3309240341",
            "type": "folder",
            "title": "Architecture documentation",
            "parentId": "3302031761",
            "parentType": "page",
            "spaceId": "3302031364",
            "status": "current",
            "authorId": "abc",
            "_links": {"webui": "/x"}
        }"#;
        let folder: CreatedFolder = serde_json::from_str(json).unwrap();
        assert_eq!(folder.id, "3309240341");
        assert_eq!(folder.title.as_deref(), Some("Architecture documentation"));
    }

    #[test]
    fn created_folder_tolerates_missing_title() {
        let folder: CreatedFolder = serde_json::from_str(r#"{"id": "1"}"#).unwrap();
        assert_eq!(folder.id, "1");
        assert!(folder.title.is_none());
    }
}
