use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::utils::ConfluenceContext;

// List spaces
pub async fn list_spaces(
    ctx: &ConfluenceContext<'_>,
    limit: Option<usize>,
    space_type: Option<&str>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct SpacesResponse {
        results: Vec<Space>,
    }

    #[derive(Deserialize)]
    struct Space {
        id: String,
        key: String,
        name: String,
        #[serde(rename = "type")]
        space_type: String,
        status: String,
    }

    let mut query_params = Vec::new();

    if let Some(l) = limit {
        query_params.push(format!("limit={}", l));
    }

    if let Some(st) = space_type {
        query_params.push(format!("type={}", st));
    }

    let query_string = if query_params.is_empty() {
        String::new()
    } else {
        format!("?{}", query_params.join("&"))
    };

    let response: SpacesResponse = ctx
        .client
        .get(&format!("/wiki/api/v2/spaces{}", query_string))
        .await
        .context("Failed to list spaces")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        key: &'a str,
        name: &'a str,
        space_type: &'a str,
        status: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .results
        .iter()
        .map(|s| Row {
            id: s.id.as_str(),
            key: s.key.as_str(),
            name: s.name.as_str(),
            space_type: s.space_type.as_str(),
            status: s.status.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Get space details
pub async fn get_space(ctx: &ConfluenceContext<'_>, key: &str) -> Result<()> {
    let space: Value = ctx
        .client
        .get(&format!("/wiki/api/v2/spaces?keys={}", key))
        .await
        .with_context(|| format!("Failed to get space {}", key))?;

    println!("{}", serde_json::to_string_pretty(&space)?);
    Ok(())
}

// Create space
pub async fn create_space(
    ctx: &ConfluenceContext<'_>,
    key: &str,
    name: &str,
    description: Option<&str>,
) -> Result<()> {
    let mut payload = json!({
        "key": key,
        "name": name,
        "type": "global",
    });

    if let Some(desc) = description {
        payload["description"] = json!({
            "plain": {
                "value": desc,
                "representation": "plain"
            }
        });
    }

    #[derive(Deserialize)]
    struct CreateResponse {
        id: String,
        key: String,
        name: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/wiki/api/v2/spaces", &payload)
        .await
        .context("Failed to create space")?;

    tracing::info!(id = %response.id, key = %response.key, "Space created successfully");
    println!("✅ Created space: {} ({})", response.name, response.key);
    Ok(())
}

// Update space
pub async fn update_space(
    ctx: &ConfluenceContext<'_>,
    space_id: &str,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    // Get current space first
    let current: Value = ctx
        .client
        .get(&format!("/wiki/api/v2/spaces/{}", space_id))
        .await
        .with_context(|| format!("Failed to get space {}", space_id))?;

    let mut payload = current;

    if let Some(n) = name {
        payload["name"] = json!(n);
    }

    if let Some(d) = description {
        payload["description"] = json!({
            "plain": {
                "value": d,
                "representation": "plain"
            }
        });
    }

    let _: Value = ctx
        .client
        .put(&format!("/wiki/api/v2/spaces/{}", space_id), &payload)
        .await
        .with_context(|| format!("Failed to update space {}", space_id))?;

    tracing::info!(%space_id, "Space updated successfully");
    println!("✅ Updated space: {}", space_id);
    Ok(())
}

// Delete space
pub async fn delete_space(ctx: &ConfluenceContext<'_>, space_id: &str, force: bool) -> Result<()> {
    if !force {
        println!(
            "⚠️  This will permanently delete space {}. Use --force to confirm.",
            space_id
        );
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!("/wiki/api/v2/spaces/{}", space_id))
        .await
        .with_context(|| format!("Failed to delete space {}", space_id))?;

    tracing::info!(%space_id, "Space deleted successfully");
    println!("✅ Deleted space: {}", space_id);
    Ok(())
}

// Get space permissions
pub async fn get_space_permissions(ctx: &ConfluenceContext<'_>, space_key: &str) -> Result<()> {
    let permissions: Value = ctx
        .client
        .get(&format!("/wiki/rest/api/space/{}/permission", space_key))
        .await
        .with_context(|| format!("Failed to get permissions for space {}", space_key))?;

    println!("{}", serde_json::to_string_pretty(&permissions)?);
    Ok(())
}

// Add space permission
pub async fn add_space_permission(
    ctx: &ConfluenceContext<'_>,
    space_key: &str,
    permission_type: &str,
    subject_type: &str,
    subject_id: &str,
) -> Result<()> {
    let payload = json!({
        "subject": {
            "type": subject_type,
            "identifier": subject_id
        },
        "operation": {
            "key": permission_type,
            "target": "space"
        }
    });

    let _: Value = ctx
        .client
        .post(
            &format!("/wiki/rest/api/space/{}/permission", space_key),
            &payload,
        )
        .await
        .with_context(|| format!("Failed to add permission to space {}", space_key))?;

    tracing::info!(%space_key, %permission_type, %subject_id, "Permission added successfully");
    println!(
        "✅ Added {} permission for {} to space {}",
        permission_type, subject_id, space_key
    );
    Ok(())
}
