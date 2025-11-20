use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::BitbucketContext;

#[derive(Deserialize)]
struct RepositoryList {
    values: Vec<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    slug: String,
    name: String,
    #[serde(default)]
    updated_on: Option<String>,
}

#[derive(Deserialize)]
struct BranchList {
    values: Vec<Branch>,
}

#[derive(Deserialize)]
struct Branch {
    name: String,
    #[serde(default)]
    target: Option<Target>,
}

#[derive(Deserialize)]
struct Target {
    #[serde(default)]
    date: Option<String>,
}

pub async fn archive_stale_repos(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    days_threshold: i64,
    dry_run: bool,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}?pagelen=100");
    let response: RepositoryList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list repositories in workspace {workspace}"))?;

    let now = chrono::Utc::now();
    let threshold = chrono::Duration::days(days_threshold);

    #[derive(Serialize)]
    struct StaleRepo<'a> {
        slug: &'a str,
        name: &'a str,
        last_updated: &'a str,
        action: &'a str,
    }

    let mut stale_repos = Vec::new();

    for repo in &response.values {
        if let Some(updated) = &repo.updated_on {
            if let Ok(updated_date) = chrono::DateTime::parse_from_rfc3339(updated) {
                let age = now.signed_duration_since(updated_date);
                if age > threshold {
                    stale_repos.push(StaleRepo {
                        slug: repo.slug.as_str(),
                        name: repo.name.as_str(),
                        last_updated: updated,
                        action: if dry_run { "would archive" } else { "archived" },
                    });

                    if !dry_run {
                        let update_path = format!("/2.0/repositories/{workspace}/{}", repo.slug);
                        let payload = serde_json::json!({
                            "has_issues": false,
                            "has_wiki": false,
                        });

                        let _: serde_json::Value = ctx
                            .client
                            .put(&update_path, &payload)
                            .await
                            .with_context(|| {
                                format!("Failed to archive repository {}", repo.slug)
                            })?;

                        tracing::info!(
                            repo_slug = repo.slug.as_str(),
                            workspace,
                            "Repository archived"
                        );
                    }
                }
            }
        }
    }

    if stale_repos.is_empty() {
        println!("No stale repositories found (threshold: {days_threshold} days)");
        return Ok(());
    }

    if dry_run {
        println!(
            "DRY RUN - No changes made. Found {} stale repositories:",
            stale_repos.len()
        );
    }

    ctx.renderer.render(&stale_repos)
}

pub async fn delete_merged_branches(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    exclude_patterns: Vec<String>,
    dry_run: bool,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/refs/branches?pagelen=100");
    let response: BranchList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list branches for {workspace}/{repo_slug}"))?;

    #[derive(Serialize)]
    struct MergedBranch<'a> {
        name: &'a str,
        last_commit: &'a str,
        action: &'a str,
    }

    let mut merged_branches = Vec::new();
    let protected = ["main", "master", "develop", "development"];

    for branch in &response.values {
        let is_protected = protected.contains(&branch.name.as_str())
            || exclude_patterns
                .iter()
                .any(|pattern| branch.name.contains(pattern));

        if !is_protected {
            merged_branches.push(MergedBranch {
                name: branch.name.as_str(),
                last_commit: branch
                    .target
                    .as_ref()
                    .and_then(|t| t.date.as_deref())
                    .unwrap_or(""),
                action: if dry_run { "would delete" } else { "deleted" },
            });

            if !dry_run {
                let delete_path = format!(
                    "/2.0/repositories/{workspace}/{repo_slug}/refs/branches/{}",
                    branch.name
                );
                let _: serde_json::Value =
                    ctx.client.delete(&delete_path).await.with_context(|| {
                        format!(
                            "Failed to delete branch {} from {workspace}/{repo_slug}",
                            branch.name
                        )
                    })?;

                tracing::info!(
                    branch_name = branch.name.as_str(),
                    workspace,
                    repo_slug,
                    "Branch deleted"
                );
            }
        }
    }

    if merged_branches.is_empty() {
        println!("No branches to delete (excluding protected patterns)");
        return Ok(());
    }

    if dry_run {
        println!(
            "DRY RUN - No changes made. Found {} branches to delete:",
            merged_branches.len()
        );
    }

    ctx.renderer.render(&merged_branches)
}
