use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::BitbucketContext;

#[derive(Deserialize)]
struct BranchList {
    values: Vec<Branch>,
}

#[derive(Deserialize)]
struct Branch {
    name: String,
    #[serde(default)]
    target: Option<Target>,
    #[serde(default)]
    default_merge_strategy: Option<String>,
}

#[derive(Deserialize)]
struct Target {
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    author: Option<Author>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct Author {
    #[serde(default)]
    raw: Option<String>,
}

#[derive(Deserialize)]
struct BranchRestriction {
    id: i64,
    kind: String,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    value: Option<i32>,
}

pub async fn list_branches(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    limit: usize,
) -> Result<()> {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("pagelen", &limit.min(100).to_string())
        .finish();
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/refs/branches?{query}");

    let response: BranchList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list branches for {workspace}/{repo_slug}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        name: &'a str,
        commit_hash: &'a str,
        author: &'a str,
        message: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|branch| Row {
            name: branch.name.as_str(),
            commit_hash: branch
                .target
                .as_ref()
                .and_then(|t| t.hash.as_deref())
                .unwrap_or("")
                .get(..7)
                .unwrap_or(""),
            author: branch
                .target
                .as_ref()
                .and_then(|t| t.author.as_ref())
                .and_then(|a| a.raw.as_deref())
                .unwrap_or(""),
            message: branch
                .target
                .as_ref()
                .and_then(|t| t.message.as_deref())
                .map(|m| m.lines().next().unwrap_or(""))
                .unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No branches returned for repository");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn get_branch(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    branch_name: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/refs/branches/{branch_name}");
    let branch: Branch = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to fetch branch {branch_name} from {workspace}/{repo_slug}")
    })?;

    #[derive(Serialize)]
    struct View<'a> {
        name: &'a str,
        commit_hash: &'a str,
        commit_message: &'a str,
        author: &'a str,
        merge_strategy: &'a str,
    }

    let view = View {
        name: branch.name.as_str(),
        commit_hash: branch
            .target
            .as_ref()
            .and_then(|t| t.hash.as_deref())
            .unwrap_or(""),
        commit_message: branch
            .target
            .as_ref()
            .and_then(|t| t.message.as_deref())
            .unwrap_or(""),
        author: branch
            .target
            .as_ref()
            .and_then(|t| t.author.as_ref())
            .and_then(|a| a.raw.as_deref())
            .unwrap_or(""),
        merge_strategy: branch.default_merge_strategy.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&view)
}

pub async fn create_branch(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    branch_name: &str,
    target: &str,
) -> Result<()> {
    let payload = serde_json::json!({
        "name": branch_name,
        "target": {
            "hash": target
        }
    });

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/refs/branches");
    let branch: Branch = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to create branch {branch_name} in {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        branch = branch.name.as_str(),
        workspace,
        repo_slug,
        "Branch created successfully"
    );

    #[derive(Serialize)]
    struct Created<'a> {
        name: &'a str,
        commit_hash: &'a str,
    }

    let created = Created {
        name: branch.name.as_str(),
        commit_hash: branch
            .target
            .as_ref()
            .and_then(|t| t.hash.as_deref())
            .unwrap_or(""),
    };

    ctx.renderer.render(&created)
}

pub async fn delete_branch(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    branch_name: &str,
    force: bool,
) -> Result<()> {
    if !force {
        use std::io::{self, Write};
        print!(
            "Are you sure you want to delete branch {branch_name} from {workspace}/{repo_slug}? [y/N]: "
        );
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            tracing::info!("Branch deletion cancelled");
            return Ok(());
        }
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/refs/branches/{branch_name}");
    let _: serde_json::Value = ctx.client.delete(&path).await.with_context(|| {
        format!("Failed to delete branch {branch_name} from {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        branch = branch_name,
        workspace,
        repo_slug,
        "Branch deleted successfully"
    );

    println!("✓ Branch {branch_name} deleted from {workspace}/{repo_slug}");
    Ok(())
}

pub async fn protect_branch(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pattern: &str,
    kind: &str,
    required_approvals: Option<i32>,
) -> Result<()> {
    let mut payload = serde_json::json!({
        "kind": kind,
        "pattern": pattern
    });

    if let Some(approvals) = required_approvals {
        payload["value"] = serde_json::json!(approvals);
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/branch-restrictions");
    let restriction: BranchRestriction =
        ctx.client.post(&path, &payload).await.with_context(|| {
            format!("Failed to add branch protection for {workspace}/{repo_slug}")
        })?;

    tracing::info!(
        restriction_id = restriction.id,
        pattern,
        kind,
        "Branch protection added successfully"
    );

    #[derive(Serialize)]
    struct Protected {
        id: i64,
        kind: String,
        pattern: String,
        required_approvals: String,
    }

    let protected = Protected {
        id: restriction.id,
        kind: restriction.kind.clone(),
        pattern: restriction.pattern.unwrap_or_default(),
        required_approvals: restriction.value.map(|v| v.to_string()).unwrap_or_default(),
    };

    ctx.renderer.render(&protected)
}

pub async fn unprotect_branch(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    restriction_id: i64,
) -> Result<()> {
    let path =
        format!("/2.0/repositories/{workspace}/{repo_slug}/branch-restrictions/{restriction_id}");
    let _: serde_json::Value = ctx.client.delete(&path).await.with_context(|| {
        format!("Failed to remove branch protection from {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        restriction_id,
        workspace,
        repo_slug,
        "Branch protection removed successfully"
    );

    println!("✓ Branch protection {restriction_id} removed from {workspace}/{repo_slug}");
    Ok(())
}

pub async fn list_restrictions(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
) -> Result<()> {
    #[derive(Deserialize)]
    struct RestrictionList {
        values: Vec<BranchRestriction>,
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/branch-restrictions");
    let response: RestrictionList = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to list branch restrictions for {workspace}/{repo_slug}")
    })?;

    #[derive(Serialize)]
    struct Row {
        id: i64,
        kind: String,
        pattern: String,
        required_approvals: String,
    }

    let rows: Vec<Row> = response
        .values
        .iter()
        .map(|r| Row {
            id: r.id,
            kind: r.kind.clone(),
            pattern: r.pattern.clone().unwrap_or_default(),
            required_approvals: r.value.map(|v| v.to_string()).unwrap_or_default(),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(
            workspace,
            repo_slug,
            "No branch restrictions for repository"
        );
        return Ok(());
    }

    ctx.renderer.render(&rows)
}
