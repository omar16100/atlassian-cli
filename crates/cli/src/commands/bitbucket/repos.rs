use anyhow::{Context, Result};
use atlassian_cli_api::pagination::{fetch_paged, BitbucketPage, PageLimits};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::{encode_path_segment, page_size, warn_if_truncated, BitbucketContext};
use crate::commands::common::{confirm_destructive, render_success, MutationResult};

#[derive(Deserialize)]
struct Repo {
    slug: String,
    name: Option<String>,
    #[serde(default)]
    is_private: bool,
    #[serde(default)]
    mainbranch: Option<BranchRef>,
    #[serde(rename = "full_name", default)]
    full_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    size: Option<i64>,
}

#[derive(Deserialize)]
struct BranchRef {
    name: String,
}

pub async fn list_repos(ctx: &BitbucketContext<'_>, workspace: &str, limit: usize) -> Result<()> {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("pagelen", &page_size(limit).to_string())
        .finish();
    let path = format!("/2.0/repositories/{workspace}?{query}");

    let (repositories, page) =
        fetch_paged::<BitbucketPage<Repo>>(&ctx.client, &path, PageLimits::from_cli_limit(limit))
            .await
            .with_context(|| format!("Failed to list repositories for workspace {workspace}"))?;
    warn_if_truncated(&page, repositories.len(), "repositories");

    #[derive(Serialize)]
    struct Row<'a> {
        slug: &'a str,
        name: &'a str,
        main_branch: &'a str,
        visibility: &'a str,
        language: &'a str,
    }

    let rows: Vec<Row<'_>> = repositories
        .iter()
        .map(|repo| Row {
            slug: repo.slug.as_str(),
            name: repo.name.as_deref().unwrap_or(""),
            main_branch: repo
                .mainbranch
                .as_ref()
                .map(|b| b.name.as_str())
                .unwrap_or(""),
            visibility: if repo.is_private { "private" } else { "public" },
            language: repo.language.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        ctx.verify_auth().await?;
        tracing::info!(workspace, "No repositories found");
    }

    ctx.renderer
        .render_list_or_empty(&rows, "No repositories found")
}

pub async fn get_repo(ctx: &BitbucketContext<'_>, workspace: &str, slug: &str) -> Result<()> {
    let path = format!(
        "/2.0/repositories/{workspace}/{}",
        encode_path_segment(slug)?
    );
    let repo: Repo = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch repository {workspace}/{slug}"))?;

    #[derive(Serialize)]
    struct View<'a> {
        slug: &'a str,
        name: &'a str,
        full_name: &'a str,
        description: &'a str,
        main_branch: &'a str,
        visibility: &'a str,
        language: &'a str,
        size_bytes: String,
    }

    let view = View {
        slug: repo.slug.as_str(),
        name: repo.name.as_deref().unwrap_or(""),
        full_name: repo.full_name.as_deref().unwrap_or(""),
        description: repo.description.as_deref().unwrap_or(""),
        main_branch: repo
            .mainbranch
            .as_ref()
            .map(|b| b.name.as_str())
            .unwrap_or(""),
        visibility: if repo.is_private { "private" } else { "public" },
        language: repo.language.as_deref().unwrap_or(""),
        size_bytes: repo.size.map(|s| s.to_string()).unwrap_or_default(),
    };

    ctx.renderer.render(&view)
}

pub async fn create_repo(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    slug: &str,
    name: Option<&str>,
    description: Option<&str>,
    is_private: bool,
    project_key: Option<&str>,
) -> Result<()> {
    let mut payload = serde_json::json!({
        "scm": "git",
        "is_private": is_private
    });

    if let Some(n) = name {
        payload["name"] = serde_json::json!(n);
    }

    if let Some(d) = description {
        payload["description"] = serde_json::json!(d);
    }

    if let Some(pk) = project_key {
        payload["project"] = serde_json::json!({"key": pk});
    }

    let path = format!(
        "/2.0/repositories/{workspace}/{}",
        encode_path_segment(slug)?
    );
    let repo: Repo = ctx
        .client
        .post(&path, &payload)
        .await
        .with_context(|| format!("Failed to create repository {workspace}/{slug}"))?;

    tracing::info!(
        slug = repo.slug.as_str(),
        workspace,
        "Repository created successfully"
    );

    #[derive(Serialize)]
    struct Created<'a> {
        slug: &'a str,
        name: &'a str,
        full_name: &'a str,
        visibility: &'a str,
    }

    let created = Created {
        slug: repo.slug.as_str(),
        name: repo.name.as_deref().unwrap_or(""),
        full_name: repo.full_name.as_deref().unwrap_or(""),
        visibility: if repo.is_private { "private" } else { "public" },
    };

    ctx.renderer.render(&created)
}

pub async fn update_repo(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    slug: &str,
    name: Option<&str>,
    description: Option<&str>,
    language: Option<&str>,
) -> Result<()> {
    let mut payload = serde_json::json!({});

    if let Some(n) = name {
        payload["name"] = serde_json::json!(n);
    }

    if let Some(d) = description {
        payload["description"] = serde_json::json!(d);
    }

    if let Some(l) = language {
        payload["language"] = serde_json::json!(l);
    }

    let path = format!(
        "/2.0/repositories/{workspace}/{}",
        encode_path_segment(slug)?
    );
    let repo: Repo = ctx
        .client
        .put(&path, &payload)
        .await
        .with_context(|| format!("Failed to update repository {workspace}/{slug}"))?;

    tracing::info!(
        slug = repo.slug.as_str(),
        workspace,
        "Repository updated successfully"
    );

    #[derive(Serialize)]
    struct Updated<'a> {
        slug: &'a str,
        name: &'a str,
        description: &'a str,
        language: &'a str,
    }

    let updated = Updated {
        slug: repo.slug.as_str(),
        name: repo.name.as_deref().unwrap_or(""),
        description: repo.description.as_deref().unwrap_or(""),
        language: repo.language.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&updated)
}

pub async fn delete_repo(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    slug: &str,
    force: bool,
) -> Result<()> {
    // Was the same bespoke prompt `bb branch delete` used to have: written to
    // **stdout**, so it corrupted `-f json`; satisfied by a bare "y"; and on
    // EOF -- a cron job with no terminal -- it cancelled and exited 0, so the
    // caller could not tell the repository still existed. Deleting a repository
    // is less recoverable than deleting a branch, and it had the weaker guard.
    if !force {
        confirm_destructive(
            slug,
            &format!(
                "About to delete the repository {workspace}/{slug}.\n\
                 This removes its code, pull requests, issues and wiki."
            ),
        )?;
    }

    // A single path segment: rejects `/`, `..` and fragments alike. Using the
    // ref helper here preserved `/`, so a slug like "myrepo/refs/branches/main"
    // reached the branch-delete endpoint and still reported a deleted
    // repository.
    let path = format!(
        "/2.0/repositories/{workspace}/{}",
        encode_path_segment(slug)?
    );
    let _: serde_json::Value = ctx
        .client
        .delete(&path)
        .await
        .with_context(|| format!("Failed to delete repository {workspace}/{slug}"))?;

    tracing::info!(slug, workspace, "Repository deleted successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Repository {workspace}/{slug} deleted"),
        &MutationResult::with_id(format!("Repository {workspace}/{slug} deleted"), slug),
    )
}
#[cfg(test)]
mod repo_delete_tests {
    use super::*;
    use atlassian_cli_api::ApiClient;
    use atlassian_cli_output::{OutputFormat, OutputRenderer};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Without a terminal and without --force, the command must refuse rather
    /// than delete or hang. The old prompt cancelled on EOF and exited 0, so a
    /// scheduled job could not tell the repository still existed.
    #[tokio::test]
    async fn a_delete_without_force_never_reaches_the_api() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = BitbucketContext {
            client: ApiClient::new(server.uri()).unwrap(),
            renderer: &renderer,
            is_bearer: false,
        };

        // Assert the safety property, not the message. Whether stdin is a
        // terminal is a property of the test runner -- under a pipe this is the
        // no-terminal refusal, under a pseudo-terminal it is a confirmation
        // mismatch on EOF. Both must reach the same outcome: nothing deleted.
        // The refusal message itself is pinned in commands::common's tests,
        // where the terminal check is injectable.
        delete_repo(&ctx, "ws", "repo", false)
            .await
            .expect_err("must not delete without --force");

        let deletes = server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.method == wiremock::http::Method::DELETE)
            .count();
        assert_eq!(deletes, 0, "nothing may be deleted when confirmation fails");
    }

    /// A slug carrying a dot segment would address a different resource than
    /// the one named in the confirmation.
    #[tokio::test]
    async fn a_traversal_slug_is_refused() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = BitbucketContext {
            client: ApiClient::new(server.uri()).unwrap(),
            renderer: &renderer,
            is_bearer: false,
        };

        // Two distinct rejections, both of which used to reach the API:
        // a slug spanning path segments (which hit the branch-delete endpoint
        // while still reporting a deleted repository), and a dot component.
        for slug in ["a/../../other/repo", "myrepo/refs/branches/main", ".."] {
            let err = delete_repo(&ctx, "ws", slug, true)
                .await
                .expect_err("slug must be refused");
            // The two guards word their refusals differently; what matters is
            // that the rejected value is named and nothing was sent.
            let message = format!("{err:#}");
            assert!(
                message.contains(slug),
                "{slug}: the error should name the value it refused: {message}"
            );
        }

        assert_eq!(
            server.received_requests().await.unwrap_or_default().len(),
            0,
            "no request may be sent for a rejected slug"
        );
    }
}
