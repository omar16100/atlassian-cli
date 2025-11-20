use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::BitbucketContext;

#[derive(Deserialize)]
struct RepoList {
    values: Vec<Repo>,
}

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
        .append_pair("pagelen", &limit.min(100).to_string())
        .finish();
    let path = format!("/2.0/repositories/{workspace}?{query}");

    let response: RepoList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list repositories for workspace {workspace}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        slug: &'a str,
        name: &'a str,
        main_branch: &'a str,
        visibility: &'a str,
        language: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
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
        tracing::info!(
            workspace,
            "No repositories returned for workspace; check permissions."
        );
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn get_repo(ctx: &BitbucketContext<'_>, workspace: &str, slug: &str) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{slug}");
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

    let path = format!("/2.0/repositories/{workspace}/{slug}");
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

    let path = format!("/2.0/repositories/{workspace}/{slug}");
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
    if !force {
        use std::io::{self, Write};
        print!("Are you sure you want to delete repository {workspace}/{slug}? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            tracing::info!("Repository deletion cancelled");
            return Ok(());
        }
    }

    let path = format!("/2.0/repositories/{workspace}/{slug}");
    let _: serde_json::Value = ctx
        .client
        .delete(&path)
        .await
        .with_context(|| format!("Failed to delete repository {workspace}/{slug}"))?;

    tracing::info!(slug, workspace, "Repository deleted successfully");

    println!("✓ Repository {workspace}/{slug} deleted");
    Ok(())
}
