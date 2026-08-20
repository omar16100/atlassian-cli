use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{BambooContext, BuildResult, ResultsContainer};

/// List builds for a plan.
pub async fn list_builds(ctx: &BambooContext<'_>, plan_key: &str, limit: usize) -> Result<()> {
    let path = format!(
        "/rest/api/latest/result/{}?max-result={}&expand=results.result",
        plan_key,
        limit.min(100)
    );
    tracing::debug!("Listing builds for plan {} with limit {}", plan_key, limit);

    let response: ResultsContainer = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list builds for plan {}", plan_key))?;

    let builds = response.results.result.unwrap_or_default();

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        number: i64,
        state: &'a str,
        lifecycle: &'a str,
        duration: i64,
        started: &'a str,
    }

    let rows: Vec<Row<'_>> = builds
        .iter()
        .map(|build| Row {
            key: &build.build_result_key,
            number: build.build_number,
            state: build
                .build_state
                .as_deref()
                .unwrap_or(build.state.as_deref().unwrap_or("")),
            lifecycle: build.life_cycle_state.as_deref().unwrap_or(""),
            duration: build.build_duration.unwrap_or(0),
            started: build.build_started_time.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        ctx.verify_auth().await?;
        tracing::info!("No builds found for plan {}", plan_key);
    }

    tracing::info!("Found {} builds for plan {}", rows.len(), plan_key);
    ctx.renderer.render_list_or_empty(&rows, "No builds found")
}

/// Get build details.
pub async fn get_build(ctx: &BambooContext<'_>, build_key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/result/{}", build_key);
    tracing::debug!("Fetching build {}", build_key);

    let build: BuildResult = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch build {}", build_key))?;

    #[derive(Serialize)]
    struct View<'a> {
        key: &'a str,
        number: i64,
        state: &'a str,
        lifecycle: &'a str,
        duration_seconds: i64,
        started: &'a str,
        completed: &'a str,
        reason: &'a str,
        tests_passed: i64,
        tests_failed: i64,
    }

    let view = View {
        key: &build.build_result_key,
        number: build.build_number,
        state: build
            .build_state
            .as_deref()
            .unwrap_or(build.state.as_deref().unwrap_or("")),
        lifecycle: build.life_cycle_state.as_deref().unwrap_or(""),
        duration_seconds: build.build_duration.unwrap_or(0),
        started: build.build_started_time.as_deref().unwrap_or(""),
        completed: build.build_completed_time.as_deref().unwrap_or(""),
        reason: build.reason_summary.as_deref().unwrap_or(""),
        tests_passed: build.successful_test_count.unwrap_or(0),
        tests_failed: build.failed_test_count.unwrap_or(0),
    };

    tracing::info!("Retrieved build {}", build_key);
    ctx.renderer.render(&view)
}

/// Get latest build for a plan.
pub async fn get_latest(ctx: &BambooContext<'_>, plan_key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/result/{}/latest", plan_key);
    tracing::debug!("Fetching latest build for plan {}", plan_key);

    let build: BuildResult = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch latest build for plan {}", plan_key))?;

    #[derive(Serialize)]
    struct View<'a> {
        key: &'a str,
        number: i64,
        state: &'a str,
        lifecycle: &'a str,
        duration_seconds: i64,
        started: &'a str,
        completed: &'a str,
    }

    let view = View {
        key: &build.build_result_key,
        number: build.build_number,
        state: build
            .build_state
            .as_deref()
            .unwrap_or(build.state.as_deref().unwrap_or("")),
        lifecycle: build.life_cycle_state.as_deref().unwrap_or(""),
        duration_seconds: build.build_duration.unwrap_or(0),
        started: build.build_started_time.as_deref().unwrap_or(""),
        completed: build.build_completed_time.as_deref().unwrap_or(""),
    };

    tracing::info!(
        "Retrieved latest build {} for plan {}",
        build.build_result_key,
        plan_key
    );
    ctx.renderer.render(&view)
}

/// Run a build for a plan.
pub async fn run_build(
    ctx: &BambooContext<'_>,
    plan_key: &str,
    stage: Option<&str>,
    custom_revision: Option<&str>,
) -> Result<()> {
    let mut path = format!("/rest/api/latest/queue/{}", plan_key);
    let mut params = vec![];

    if let Some(s) = stage {
        params.push(format!("stage={}", s));
    }
    if let Some(r) = custom_revision {
        params.push(format!("customRevision={}", r));
    }

    if !params.is_empty() {
        path = format!("{}?{}", path, params.join("&"));
    }

    tracing::debug!("Queueing build for plan {}", plan_key);

    #[derive(serde::Deserialize)]
    struct QueueResponse {
        #[serde(rename = "buildResultKey")]
        build_result_key: String,
        #[serde(rename = "buildNumber")]
        build_number: i64,
    }

    let response: QueueResponse = ctx
        .client
        .post(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to queue build for plan {}", plan_key))?;

    tracing::info!(
        "Queued build {} for plan {}",
        response.build_result_key,
        plan_key
    );
    println!(
        "Build queued: {} (build #{})",
        response.build_result_key, response.build_number
    );
    Ok(())
}

/// Stop a running build.
pub async fn stop_build(ctx: &BambooContext<'_>, build_key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/queue/{}", build_key);

    tracing::debug!("Stopping build {}", build_key);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to stop build {}", build_key))?;

    tracing::info!("Stopped build {}", build_key);
    println!("Successfully stopped build {}", build_key);
    Ok(())
}

/// Get build logs.
pub async fn get_logs(
    ctx: &BambooContext<'_>,
    build_key: &str,
    job_key: Option<&str>,
) -> Result<()> {
    let path = if let Some(job) = job_key {
        format!("/rest/api/latest/result/{}/{}/log", build_key, job)
    } else {
        format!("/rest/api/latest/result/{}/log", build_key)
    };

    tracing::debug!("Fetching logs for build {}", build_key);

    let logs = ctx
        .client
        .get_text(&path)
        .await
        .with_context(|| format!("Failed to fetch logs for build {}", build_key))?;

    tracing::info!("Retrieved logs for build {}", build_key);
    println!("{}", logs);
    Ok(())
}

/// Add comment to a build.
pub async fn add_comment(ctx: &BambooContext<'_>, build_key: &str, content: &str) -> Result<()> {
    #[derive(serde::Serialize)]
    struct CommentBody<'a> {
        content: &'a str,
    }

    let path = format!("/rest/api/latest/result/{}/comment", build_key);
    let body = CommentBody { content };

    tracing::debug!("Adding comment to build {}", build_key);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add comment to build {}", build_key))?;

    tracing::info!("Added comment to build {}", build_key);
    println!("Successfully added comment to build {}", build_key);
    Ok(())
}

/// Add label to a build.
pub async fn add_label(ctx: &BambooContext<'_>, build_key: &str, label: &str) -> Result<()> {
    #[derive(serde::Serialize)]
    struct LabelBody<'a> {
        name: &'a str,
    }

    let path = format!("/rest/api/latest/result/{}/label", build_key);
    let body = LabelBody { name: label };

    tracing::debug!("Adding label '{}' to build {}", label, build_key);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add label to build {}", build_key))?;

    tracing::info!("Added label '{}' to build {}", label, build_key);
    println!(
        "Successfully added label '{}' to build {}",
        label, build_key
    );
    Ok(())
}

/// Remove label from a build.
pub async fn remove_label(ctx: &BambooContext<'_>, build_key: &str, label: &str) -> Result<()> {
    let path = format!("/rest/api/latest/result/{}/label/{}", build_key, label);

    tracing::debug!("Removing label '{}' from build {}", label, build_key);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to remove label from build {}", build_key))?;

    tracing::info!("Removed label '{}' from build {}", label, build_key);
    println!(
        "Successfully removed label '{}' from build {}",
        label, build_key
    );
    Ok(())
}
