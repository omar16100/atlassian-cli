use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::BitbucketContext;

#[derive(Deserialize)]
struct PermissionList {
    values: Vec<Permission>,
}

#[derive(Deserialize)]
struct Permission {
    #[serde(default)]
    user: Option<User>,
    #[serde(default)]
    group: Option<Group>,
    permission: String,
}

#[derive(Deserialize)]
struct User {
    #[serde(rename = "display_name")]
    display_name: String,
    #[serde(default)]
    uuid: Option<String>,
}

#[derive(Deserialize)]
struct Group {
    name: String,
    slug: String,
}

pub async fn list_repo_permissions(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/permissions");

    let response: PermissionList = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to list permissions for repository {workspace}/{repo_slug}")
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        entity_type: &'a str,
        entity_name: &'a str,
        permission: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|perm| {
            if let Some(user) = &perm.user {
                Row {
                    entity_type: "user",
                    entity_name: user.display_name.as_str(),
                    permission: perm.permission.as_str(),
                }
            } else if let Some(group) = &perm.group {
                Row {
                    entity_type: "group",
                    entity_name: group.name.as_str(),
                    permission: perm.permission.as_str(),
                }
            } else {
                Row {
                    entity_type: "unknown",
                    entity_name: "",
                    permission: perm.permission.as_str(),
                }
            }
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No permissions found for repository");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn grant_repo_permission(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    user_uuid: &str,
    permission: &str,
) -> Result<()> {
    let payload = serde_json::json!({
        "permission": permission
    });

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/permissions/{user_uuid}");
    let _: serde_json::Value = ctx.client.put(&path, &payload).await.with_context(|| {
        format!("Failed to grant permission to user {user_uuid} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        user_uuid,
        permission,
        workspace,
        repo_slug,
        "Permission granted successfully"
    );

    println!("✓ Granted {permission} permission to user {user_uuid} on {workspace}/{repo_slug}");
    Ok(())
}

pub async fn revoke_repo_permission(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    user_uuid: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/permissions/{user_uuid}");
    let _: serde_json::Value = ctx.client.delete(&path).await.with_context(|| {
        format!("Failed to revoke permission from user {user_uuid} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        user_uuid,
        workspace,
        repo_slug,
        "Permission revoked successfully"
    );

    println!("✓ Revoked permission from user {user_uuid} on {workspace}/{repo_slug}");
    Ok(())
}
