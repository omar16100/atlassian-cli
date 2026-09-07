//! Repository permissions.
//!
//! Every function here used to request `/permissions`, which is not an endpoint
//! Bitbucket serves: all three returned 404 "Resource not found". The real
//! resource is `permissions-config`, and it is split by principal type —
//! `permissions-config/users` and `permissions-config/groups` — so listing has
//! to read both and merge them.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::{encode_ref_path, BitbucketContext};
use crate::commands::common::{render_success, MutationResult};

#[derive(Deserialize)]
struct PermissionList {
    #[serde(default)]
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
    #[serde(default)]
    account_id: Option<String>,
}

#[derive(Deserialize)]
struct Group {
    name: String,
    #[serde(default)]
    slug: Option<String>,
}

/// One row of the merged listing.
///
/// `id` exists because of a specific complaint: `pr create --reviewers` demands
/// UUIDs and nothing in the CLI could produce one. This listing is the natural
/// place to find them, so the output of this command is now valid input to that
/// one.
#[derive(Serialize)]
struct Row {
    entity_type: &'static str,
    entity_name: String,
    id: String,
    permission: String,
}

fn user_row(perm: &Permission, user: &User) -> Row {
    Row {
        entity_type: "user",
        entity_name: user.display_name.clone(),
        // The UUID is what the reviewer flags take; the account id is the
        // fallback for accounts that withhold it.
        id: user
            .uuid
            .clone()
            .or_else(|| user.account_id.clone())
            .unwrap_or_default(),
        permission: perm.permission.clone(),
    }
}

fn group_row(perm: &Permission, group: &Group) -> Row {
    Row {
        entity_type: "group",
        entity_name: group.name.clone(),
        id: group.slug.clone().unwrap_or_default(),
        permission: perm.permission.clone(),
    }
}

/// Turn one `permissions-config` page into rows, ignoring entries that name
/// neither a user nor a group.
fn rows_from(list: &PermissionList) -> Vec<Row> {
    list.values
        .iter()
        .filter_map(|perm| match (&perm.user, &perm.group) {
            (Some(user), _) => Some(user_row(perm, user)),
            (_, Some(group)) => Some(group_row(perm, group)),
            _ => None,
        })
        .collect()
}

pub async fn list_repo_permissions(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
) -> Result<()> {
    let base = format!("/2.0/repositories/{workspace}/{repo_slug}/permissions-config");

    let users: PermissionList = ctx
        .client
        .get(&format!("{base}/users"))
        .await
        .with_context(|| {
            format!("Failed to list user permissions for repository {workspace}/{repo_slug}")
        })?;

    let groups: PermissionList = ctx
        .client
        .get(&format!("{base}/groups"))
        .await
        .with_context(|| {
            format!("Failed to list group permissions for repository {workspace}/{repo_slug}")
        })?;

    let mut rows = rows_from(&users);
    rows.extend(rows_from(&groups));

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No permissions found");
    }

    ctx.renderer
        .render_list_or_empty(&rows, "No permissions found")
}

/// Path for one principal's permission entry.
///
/// The id is percent-encoded: Bitbucket user UUIDs are brace-wrapped
/// (`{d6a3...}`), and braces are not path characters. Leaving them to be
/// encoded implicitly means the request depends on URL-library behaviour rather
/// than on something this code states.
fn user_permission_path(workspace: &str, repo_slug: &str, user_id: &str) -> Result<String> {
    Ok(format!(
        "/2.0/repositories/{workspace}/{repo_slug}/permissions-config/users/{}",
        encode_ref_path(user_id)?
    ))
}

pub async fn grant_repo_permission(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    user_uuid: &str,
    permission: &str,
) -> Result<()> {
    let payload = serde_json::json!({ "permission": permission });

    let path = user_permission_path(workspace, repo_slug, user_uuid)?;
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

    render_success(
        ctx.renderer,
        &format!(
            "✅ Granted {permission} permission to user {user_uuid} on {workspace}/{repo_slug}"
        ),
        &MutationResult::new(format!(
            "Granted {permission} permission to user {user_uuid}"
        )),
    )
}

pub async fn revoke_repo_permission(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    user_uuid: &str,
) -> Result<()> {
    let path = user_permission_path(workspace, repo_slug, user_uuid)?;
    let _: serde_json::Value = ctx.client.delete(&path).await.with_context(|| {
        format!("Failed to revoke permission from user {user_uuid} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        user_uuid,
        workspace,
        repo_slug,
        "Permission revoked successfully"
    );

    render_success(
        ctx.renderer,
        &format!("✅ Revoked permission from user {user_uuid} on {workspace}/{repo_slug}"),
        &MutationResult::new(format!("Revoked permission from user {user_uuid}")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlassian_cli_api::ApiClient;
    use atlassian_cli_output::{OutputFormat, OutputRenderer};
    use wiremock::matchers::{method, path as path_matcher};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// The whole point of the change: not `/permissions`.
    #[test]
    fn the_permission_path_is_permissions_config() {
        let path = user_permission_path("ws", "repo", "abc").unwrap();
        assert_eq!(
            path,
            "/2.0/repositories/ws/repo/permissions-config/users/abc"
        );
        assert!(!path.contains("/permissions/"));
    }

    /// Bitbucket UUIDs are brace-wrapped, and braces are not path characters.
    #[test]
    fn a_brace_wrapped_uuid_is_encoded() {
        let path = user_permission_path("ws", "repo", "{d6a3f1-22}").unwrap();
        assert!(path.ends_with("/users/%7Bd6a3f1-22%7D"), "{path}");
    }

    #[test]
    fn users_and_groups_both_become_rows() {
        let list: PermissionList = serde_json::from_value(serde_json::json!({
            "values": [
                {"permission": "admin", "user": {"display_name": "Benji", "uuid": "{u-1}"}},
                {"permission": "read", "group": {"name": "Devs", "slug": "devs"}},
            ]
        }))
        .unwrap();

        let rows = rows_from(&list);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].entity_type, "user");
        assert_eq!(rows[0].id, "{u-1}");
        assert_eq!(rows[1].entity_type, "group");
        assert_eq!(rows[1].id, "devs");
    }

    /// A user who withholds their UUID still has an account id, and that is
    /// better than an empty column.
    #[test]
    fn account_id_is_the_fallback_when_uuid_is_absent() {
        let list: PermissionList = serde_json::from_value(serde_json::json!({
            "values": [{
                "permission": "write",
                "user": {"display_name": "Sam", "account_id": "acc-9"}
            }]
        }))
        .unwrap();

        assert_eq!(rows_from(&list)[0].id, "acc-9");
    }

    #[test]
    fn entries_naming_neither_principal_are_dropped() {
        let list: PermissionList = serde_json::from_value(serde_json::json!({
            "values": [{"permission": "read"}]
        }))
        .unwrap();

        assert!(rows_from(&list).is_empty());
    }

    /// Listing must read both collections. Reading only one silently omits
    /// every group, which is how a repository looks unprotected when it is not.
    #[tokio::test]
    async fn listing_reads_both_users_and_groups() {
        let server = MockServer::start().await;
        let base = "/2.0/repositories/ws/repo/permissions-config";

        Mock::given(method("GET"))
            .and(path_matcher(format!("{base}/users")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [{"permission": "admin",
                            "user": {"display_name": "Benji", "uuid": "{u-1}"}}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_matcher(format!("{base}/groups")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [{"permission": "read",
                            "group": {"name": "Devs", "slug": "devs"}}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = BitbucketContext {
            client: ApiClient::new(server.uri()).unwrap(),
            renderer: &renderer,
            is_bearer: false,
        };

        list_repo_permissions(&ctx, "ws", "repo").await.unwrap();
        // `expect(1)` on both mocks is asserted when the server drops.
    }
}
