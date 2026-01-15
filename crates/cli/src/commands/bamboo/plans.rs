use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{BambooContext, ListResponse, Plan, PlansWrapper};

/// List all plans.
pub async fn list_plans(ctx: &BambooContext<'_>, limit: usize) -> Result<()> {
    let path = format!("/rest/api/latest/plan?max-result={}", limit.min(100));
    tracing::debug!("Listing plans with limit {}", limit);

    #[derive(serde::Deserialize)]
    struct Response {
        plans: ListResponse<PlansWrapper>,
    }

    let response: Response = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list plans")?;

    let plans = response.plans.items.plan.unwrap_or_default();

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        name: &'a str,
        project_key: &'a str,
        enabled: bool,
        building: bool,
    }

    let rows: Vec<Row<'_>> = plans
        .iter()
        .map(|plan| Row {
            key: &plan.key,
            name: &plan.name,
            project_key: plan.project_key.as_deref().unwrap_or(""),
            enabled: plan.enabled.unwrap_or(true),
            building: plan.is_building.unwrap_or(false),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No plans found");
        println!("No plans found");
        return Ok(());
    }

    tracing::info!("Found {} plans", rows.len());
    ctx.renderer.render(&rows)
}

/// Get plan details.
pub async fn get_plan(ctx: &BambooContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}", key);
    tracing::debug!("Fetching plan {}", key);

    let plan: Plan = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch plan {}", key))?;

    #[derive(Serialize)]
    struct View<'a> {
        key: &'a str,
        name: &'a str,
        description: &'a str,
        project_key: &'a str,
        project_name: &'a str,
        enabled: bool,
        building: bool,
        avg_build_time: i64,
    }

    let view = View {
        key: &plan.key,
        name: &plan.name,
        description: plan.description.as_deref().unwrap_or(""),
        project_key: plan.project_key.as_deref().unwrap_or(""),
        project_name: plan.project_name.as_deref().unwrap_or(""),
        enabled: plan.enabled.unwrap_or(true),
        building: plan.is_building.unwrap_or(false),
        avg_build_time: plan.average_build_time.unwrap_or(0),
    };

    tracing::info!("Retrieved plan {}", key);
    ctx.renderer.render(&view)
}

/// Enable a plan.
pub async fn enable_plan(ctx: &BambooContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}/enable", key);

    tracing::debug!("Enabling plan {}", key);
    ctx.client
        .post::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to enable plan {}", key))?;

    tracing::info!("Enabled plan {}", key);
    println!("Successfully enabled plan {}", key);
    Ok(())
}

/// Disable a plan.
pub async fn disable_plan(ctx: &BambooContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}/disable", key);

    tracing::debug!("Disabling plan {}", key);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to disable plan {}", key))?;

    tracing::info!("Disabled plan {}", key);
    println!("Successfully disabled plan {}", key);
    Ok(())
}

/// Favorite a plan.
pub async fn favorite_plan(ctx: &BambooContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}/favourite", key);

    tracing::debug!("Favoriting plan {}", key);
    ctx.client
        .post::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to favorite plan {}", key))?;

    tracing::info!("Favorited plan {}", key);
    println!("Successfully favorited plan {}", key);
    Ok(())
}

/// Unfavorite a plan.
pub async fn unfavorite_plan(ctx: &BambooContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/plan/{}/favourite", key);

    tracing::debug!("Unfavoriting plan {}", key);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to unfavorite plan {}", key))?;

    tracing::info!("Unfavorited plan {}", key);
    println!("Successfully unfavorited plan {}", key);
    Ok(())
}
