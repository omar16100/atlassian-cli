use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{BambooContext, Branch, BranchesWrapper, ListResponse};

/// List branches for a plan.
pub async fn list_branches(ctx: &BambooContext<'_>, plan_key: &str, limit: usize) -> Result<()> {
    let path = format!(
        "/rest/api/latest/plan/{}/branch?max-result={}",
        plan_key,
        limit.min(100)
    );
    tracing::debug!(
        "Listing branches for plan {} with limit {}",
        plan_key,
        limit
    );

    #[derive(serde::Deserialize)]
    struct Response {
        branches: ListResponse<BranchesWrapper>,
    }

    let response: Response = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list branches for plan {}", plan_key))?;

    let branches = response.branches.items.branch.unwrap_or_default();

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        name: &'a str,
        enabled: bool,
    }

    let rows: Vec<Row<'_>> = branches
        .iter()
        .map(|branch| Row {
            key: &branch.key,
            name: &branch.name,
            enabled: branch.enabled.unwrap_or(true),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No branches found for plan {}", plan_key);
    }

    tracing::info!("Found {} branches for plan {}", rows.len(), plan_key);
    ctx.renderer
        .render_list_or_empty(&rows, "No branches found")
}

/// Get branch details.
pub async fn get_branch(ctx: &BambooContext<'_>, branch_key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}", branch_key);
    tracing::debug!("Fetching branch {}", branch_key);

    let branch: Branch = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch branch {}", branch_key))?;

    #[derive(Serialize)]
    struct View<'a> {
        key: &'a str,
        name: &'a str,
        description: &'a str,
        enabled: bool,
    }

    let view = View {
        key: &branch.key,
        name: &branch.name,
        description: branch.description.as_deref().unwrap_or(""),
        enabled: branch.enabled.unwrap_or(true),
    };

    tracing::info!("Retrieved branch {}", branch_key);
    ctx.renderer.render(&view)
}

/// Create a branch plan.
pub async fn create_branch(
    ctx: &BambooContext<'_>,
    plan_key: &str,
    branch_name: &str,
    vcs_branch: Option<&str>,
) -> Result<()> {
    #[derive(serde::Serialize)]
    struct CreateBranchBody<'a> {
        #[serde(rename = "branchName")]
        branch_name: &'a str,
        #[serde(rename = "vcsBranch", skip_serializing_if = "Option::is_none")]
        vcs_branch: Option<&'a str>,
    }

    let path = format!("/rest/api/latest/plan/{}/branch", plan_key);
    let body = CreateBranchBody {
        branch_name,
        vcs_branch,
    };

    tracing::debug!("Creating branch {} for plan {}", branch_name, plan_key);

    #[derive(serde::Deserialize)]
    struct CreateResult {
        key: String,
    }

    let response: CreateResult = ctx.client.put(&path, &body).await.with_context(|| {
        format!(
            "Failed to create branch {} for plan {}",
            branch_name, plan_key
        )
    })?;

    tracing::info!("Created branch {} (key: {})", branch_name, response.key);
    println!("Created branch: {} (key: {})", branch_name, response.key);
    Ok(())
}

/// Delete a branch plan.
pub async fn delete_branch(ctx: &BambooContext<'_>, branch_key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}", branch_key);

    tracing::debug!("Deleting branch {}", branch_key);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to delete branch {}", branch_key))?;

    tracing::info!("Deleted branch {}", branch_key);
    println!("Successfully deleted branch {}", branch_key);
    Ok(())
}

/// Enable a branch plan.
pub async fn enable_branch(ctx: &BambooContext<'_>, branch_key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}/enable", branch_key);

    tracing::debug!("Enabling branch {}", branch_key);
    ctx.client
        .post::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to enable branch {}", branch_key))?;

    tracing::info!("Enabled branch {}", branch_key);
    println!("Successfully enabled branch {}", branch_key);
    Ok(())
}

/// Disable a branch plan.
pub async fn disable_branch(ctx: &BambooContext<'_>, branch_key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}/disable", branch_key);

    tracing::debug!("Disabling branch {}", branch_key);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to disable branch {}", branch_key))?;

    tracing::info!("Disabled branch {}", branch_key);
    println!("Successfully disabled branch {}", branch_key);
    Ok(())
}
