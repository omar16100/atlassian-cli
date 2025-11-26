use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use atlassian_cli_output::OutputFormat;
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::BitbucketContext;

// ============================================================================
// API Response Structs
// ============================================================================

#[derive(Deserialize)]
struct PipelineList {
    values: Vec<Pipeline>,
    next: Option<String>,
    #[allow(dead_code)]
    page: Option<u32>,
    #[allow(dead_code)]
    pagelen: Option<u32>,
    #[allow(dead_code)]
    size: Option<u32>,
}

#[derive(Deserialize, Clone)]
struct Pipeline {
    uuid: String,
    #[serde(default)]
    build_number: Option<i64>,
    #[serde(default)]
    state: Option<PipelineState>,
    #[serde(default)]
    created_on: Option<String>,
    #[serde(default)]
    completed_on: Option<String>,
    #[serde(default)]
    target: Option<Target>,
}

#[derive(Deserialize, Clone)]
struct PipelineState {
    name: String,
    #[serde(default)]
    result: Option<StateResult>,
}

#[derive(Deserialize, Clone)]
struct StateResult {
    name: String,
}

#[derive(Deserialize, Clone)]
struct Target {
    #[serde(default)]
    ref_name: Option<String>,
    #[serde(rename = "type", default)]
    target_type: Option<String>,
}

#[derive(Deserialize)]
struct StepList {
    values: Vec<PipelineStep>,
}

#[derive(Deserialize, Clone)]
struct PipelineStep {
    uuid: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    state: Option<StepState>,
}

#[derive(Deserialize, Clone)]
struct StepState {
    name: String,
    #[serde(default)]
    result: Option<StepResult>,
}

#[derive(Deserialize, Clone)]
struct StepResult {
    name: String,
}

// ============================================================================
// Output Structs
// ============================================================================

#[derive(Serialize)]
struct PipelineRow {
    build_number: String,
    state: String,
    ref_name: String,
    target_type: String,
    created: String,
}

#[derive(Serialize)]
struct PipelineView {
    uuid: String,
    build_number: String,
    state: String,
    ref_name: String,
    created: String,
    completed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps: Option<Vec<StepInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps_summary: Option<String>,
}

#[derive(Serialize, Clone)]
struct StepInfo {
    name: String,
    status: String,
}

#[derive(Serialize)]
struct LogsView {
    url: String,
    note: String,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn get_status_icon(status: &str) -> &'static str {
    match status.to_uppercase().as_str() {
        "SUCCESSFUL" | "COMPLETED" => "✅",
        "IN_PROGRESS" | "RUNNING" => "🔄",
        "FAILED" | "ERROR" => "❌",
        "STOPPED" => "⏹",
        "PENDING" | "NOT_RUN" => "⏳",
        "PAUSED" => "⏸",
        _ => "❓",
    }
}

fn get_step_status(step: &PipelineStep) -> String {
    step.state
        .as_ref()
        .and_then(|s| s.result.as_ref().map(|r| r.name.clone()))
        .or_else(|| step.state.as_ref().map(|s| s.name.clone()))
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn get_pipeline_status(pipeline: &Pipeline) -> String {
    pipeline
        .state
        .as_ref()
        .and_then(|s| s.result.as_ref().map(|r| r.name.clone()))
        .or_else(|| pipeline.state.as_ref().map(|s| s.name.clone()))
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn is_terminal_state(status: &str) -> bool {
    matches!(
        status.to_uppercase().as_str(),
        "SUCCESSFUL" | "FAILED" | "STOPPED" | "ERROR" | "EXPIRED" | "COMPLETED"
    )
}

fn format_steps_summary(steps: &[StepInfo]) -> String {
    steps
        .iter()
        .map(|s| format!("{} {}", s.name, get_status_icon(&s.status)))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn format_elapsed(start: Instant) -> String {
    let elapsed = start.elapsed();
    let secs = elapsed.as_secs();
    let mins = secs / 60;
    let hours = mins / 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", mins, secs % 60)
    }
}

// Valid sort fields for pipeline list
const VALID_SORT_FIELDS: &[&str] = &[
    "created_on",
    "-created_on",
    "updated_on",
    "-updated_on",
    "build_number",
    "-build_number",
    "state.name",
    "-state.name",
];

fn validate_sort_field(sort: &str) -> Result<()> {
    if !VALID_SORT_FIELDS.contains(&sort) {
        anyhow::bail!(
            "Invalid sort field '{}'. Valid options: {}",
            sort,
            VALID_SORT_FIELDS.join(", ")
        );
    }
    Ok(())
}

fn build_request_path(
    next_url: &Option<String>,
    workspace: &str,
    repo_slug: &str,
    page_size: usize,
    sort: &str,
    branch: Option<&str>,
) -> String {
    if let Some(url) = next_url {
        url.strip_prefix("https://api.bitbucket.org")
            .unwrap_or(url)
            .to_string()
    } else {
        let mut query = form_urlencoded::Serializer::new(String::new());
        query.append_pair("pagelen", &page_size.to_string());
        query.append_pair("sort", sort);
        if let Some(b) = branch.filter(|s| !s.is_empty()) {
            // Bitbucket requires q= filter syntax for filtering by branch
            query.append_pair("q", &format!("target.ref_name=\"{}\"", b));
        }
        format!(
            "/2.0/repositories/{workspace}/{repo_slug}/pipelines?{}",
            query.finish()
        )
    }
}

/// Resolve pipeline identifier: build number (e.g. "404") -> UUID
async fn resolve_pipeline_identifier(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    identifier: &str,
) -> Result<String> {
    // If looks like UUID (contains braces or hyphens) or not all digits, use directly
    if identifier.contains('{')
        || identifier.contains('-')
        || !identifier.chars().all(|c| c.is_ascii_digit())
    {
        return Ok(identifier.to_string());
    }

    // Numeric: resolve build number
    let build_num: i64 = identifier
        .parse()
        .with_context(|| format!("Invalid pipeline identifier: {identifier}"))?;

    tracing::debug!(build_num, "Resolving build number to UUID");

    // Try direct filter first: q=build_number=<n>
    let filter_path = format!(
        "/2.0/repositories/{workspace}/{repo_slug}/pipelines?q=build_number%3D{build_num}&pagelen=1"
    );

    if let Ok(response) = ctx.client.get::<PipelineList>(&filter_path).await {
        if let Some(pipeline) = response.values.into_iter().next() {
            if pipeline.build_number == Some(build_num) {
                tracing::debug!(build_num, uuid = %pipeline.uuid, "Resolved via direct filter");
                return Ok(pipeline.uuid);
            }
        }
    }

    // Fallback: paginate newest-first with page budget
    tracing::debug!(
        build_num,
        "Direct filter failed, falling back to pagination"
    );
    let mut next_url: Option<String> = None;
    let base_path =
        format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines?sort=-created_on&pagelen=100");
    const MAX_PAGES: usize = 10; // Budget: 1000 pipelines max

    for _page in 0..MAX_PAGES {
        let path = next_url
            .as_ref()
            .map(|u| {
                u.strip_prefix("https://api.bitbucket.org")
                    .unwrap_or(u)
                    .to_string()
            })
            .unwrap_or_else(|| base_path.clone());

        let response: PipelineList = ctx.client.get(&path).await.with_context(|| {
            format!("Failed to list pipelines when resolving build number {build_num}")
        })?;

        for pipeline in response.values {
            if pipeline.build_number == Some(build_num) {
                tracing::debug!(build_num, uuid = %pipeline.uuid, "Resolved via pagination");
                return Ok(pipeline.uuid);
            }
        }

        match response.next {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }

    anyhow::bail!(
        "Pipeline #{build_num} not found in recent 1000 pipelines. Use UUID for older builds."
    )
}

// ============================================================================
// API Functions
// ============================================================================

async fn fetch_steps(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
) -> Result<Vec<StepInfo>> {
    let path =
        format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/");
    let response: StepList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch steps for pipeline {pipeline_uuid}"))?;

    Ok(response
        .values
        .iter()
        .map(|step| StepInfo {
            name: step.name.clone().unwrap_or_else(|| step.uuid.clone()),
            status: get_step_status(step),
        })
        .collect())
}

async fn fetch_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
) -> Result<Pipeline> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}");
    ctx.client.get(&path).await.with_context(|| {
        format!("Failed to fetch pipeline {pipeline_uuid} for {workspace}/{repo_slug}")
    })
}

// ============================================================================
// Command Implementations
// ============================================================================

#[allow(clippy::too_many_arguments)]
pub async fn list_pipelines(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    limit: usize,
    sort: Option<&str>,
    recent: Option<usize>,
    branch: Option<&str>,
    fetch_all: bool,
) -> Result<()> {
    // Handle --recent shorthand
    let (effective_limit, effective_sort) = if let Some(n) = recent {
        (n, "-created_on")
    } else {
        (limit, sort.unwrap_or("-created_on"))
    };

    // Validate sort field
    validate_sort_field(effective_sort)?;

    // max_items: None = unlimited (--all or --limit 0), Some(n) = cap at n
    let max_items: Option<usize> = if fetch_all || effective_limit == 0 {
        None
    } else {
        Some(effective_limit)
    };

    let mut all_pipelines: Vec<Pipeline> = Vec::new();
    let mut next_url: Option<String> = None;
    let page_size = 100; // Max allowed by Bitbucket API

    loop {
        let path = build_request_path(
            &next_url,
            workspace,
            repo_slug,
            page_size,
            effective_sort,
            branch,
        );

        let response: PipelineList = ctx
            .client
            .get(&path)
            .await
            .with_context(|| format!("Failed to list pipelines for {workspace}/{repo_slug}"))?;

        all_pipelines.extend(response.values);
        next_url = response.next;

        // Stop if: no more pages OR reached limit (when not unlimited)
        let reached_limit = max_items.map(|m| all_pipelines.len() >= m).unwrap_or(false);
        if next_url.is_none() || reached_limit {
            break;
        }
    }

    // Truncate to exact limit
    if let Some(max) = max_items {
        if all_pipelines.len() > max {
            all_pipelines.truncate(max);
        }
    }

    let rows: Vec<PipelineRow> = all_pipelines
        .iter()
        .map(|pipeline| PipelineRow {
            build_number: pipeline
                .build_number
                .map(|n| n.to_string())
                .unwrap_or_default(),
            state: get_pipeline_status(pipeline),
            ref_name: pipeline
                .target
                .as_ref()
                .and_then(|t| t.ref_name.clone())
                .unwrap_or_default(),
            target_type: pipeline
                .target
                .as_ref()
                .and_then(|t| t.target_type.clone())
                .unwrap_or_default(),
            created: pipeline.created_on.clone().unwrap_or_default(),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No pipelines found for repository");
        return Ok(());
    }

    tracing::debug!(workspace, repo_slug, count = rows.len(), "Listed pipelines");

    ctx.renderer.render(&rows)
}

pub async fn get_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_id: &str,
    show_steps: bool,
) -> Result<()> {
    // Resolve build number to UUID if needed
    let pipeline_uuid = resolve_pipeline_identifier(ctx, workspace, repo_slug, pipeline_id).await?;
    let pipeline = fetch_pipeline(ctx, workspace, repo_slug, &pipeline_uuid).await?;

    let steps = if show_steps {
        Some(fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid).await?)
    } else {
        None
    };

    // Only include steps_summary if steps is non-empty
    let steps_summary = steps
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| format_steps_summary(s));
    let state = get_pipeline_status(&pipeline);

    let view = PipelineView {
        uuid: pipeline.uuid,
        build_number: pipeline
            .build_number
            .map(|n| n.to_string())
            .unwrap_or_default(),
        state,
        ref_name: pipeline
            .target
            .as_ref()
            .and_then(|t| t.ref_name.clone())
            .unwrap_or_default(),
        created: pipeline.created_on.unwrap_or_default(),
        completed: pipeline.completed_on.unwrap_or_default(),
        steps,
        steps_summary,
    };

    ctx.renderer.render(&view)
}

pub async fn trigger_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    ref_name: &str,
    ref_type: &str,
) -> Result<()> {
    let payload = serde_json::json!({
        "target": {
            "ref_name": ref_name,
            "ref_type": ref_type,
            "type": "pipeline_ref_target"
        }
    });

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/");
    let pipeline: Pipeline = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to trigger pipeline for {ref_name} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        build_number = pipeline.build_number,
        ref_name,
        workspace,
        repo_slug,
        "Pipeline triggered successfully"
    );

    #[derive(Serialize)]
    struct Triggered {
        uuid: String,
        build_number: String,
        state: String,
        ref_name: String,
    }

    let state = get_pipeline_status(&pipeline);
    let triggered = Triggered {
        uuid: pipeline.uuid,
        build_number: pipeline
            .build_number
            .map(|n| n.to_string())
            .unwrap_or_default(),
        state,
        ref_name: pipeline
            .target
            .as_ref()
            .and_then(|t| t.ref_name.clone())
            .unwrap_or_default(),
    };

    ctx.renderer.render(&triggered)
}

pub async fn stop_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
) -> Result<()> {
    let path =
        format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/stopPipeline");
    let _: serde_json::Value = ctx
        .client
        .post(&path, &serde_json::json!({}))
        .await
        .with_context(|| {
            format!("Failed to stop pipeline {pipeline_uuid} on {workspace}/{repo_slug}")
        })?;

    tracing::info!(
        pipeline_uuid,
        workspace,
        repo_slug,
        "Pipeline stopped successfully"
    );

    // Only print human-readable message for table output
    if ctx.renderer.format() == OutputFormat::Table {
        println!("✓ Pipeline {pipeline_uuid} stopped on {workspace}/{repo_slug}");
    } else {
        #[derive(Serialize)]
        struct StopResult {
            success: bool,
            pipeline_uuid: String,
            message: String,
        }
        ctx.renderer.render(&StopResult {
            success: true,
            pipeline_uuid: pipeline_uuid.to_string(),
            message: format!("Pipeline stopped on {workspace}/{repo_slug}"),
        })?;
    }

    Ok(())
}

pub async fn get_pipeline_logs(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
    step_uuid: &str,
) -> Result<()> {
    tracing::info!(
        pipeline_uuid,
        step_uuid,
        workspace,
        repo_slug,
        "Fetching pipeline logs"
    );

    let url = format!(
        "https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{}/steps/{}",
        pipeline_uuid.trim_matches('{').trim_matches('}'),
        step_uuid.trim_matches('{').trim_matches('}')
    );

    // Return structured output for JSON/YAML/CSV, human-readable for table
    if ctx.renderer.format() == OutputFormat::Table || ctx.renderer.format() == OutputFormat::Quiet
    {
        println!("Pipeline logs for step {step_uuid}:");
        println!("View at: {url}");
        println!("\nNote: Use the web interface to view full logs with syntax highlighting");
        Ok(())
    } else {
        ctx.renderer.render(&LogsView {
            url,
            note: "Use the web interface to view full logs with syntax highlighting".to_string(),
        })
    }
}

pub async fn watch_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_id: &str,
    interval: u64,
    show_steps: bool,
) -> Result<()> {
    // Resolve build number to UUID if needed (only once at start)
    let pipeline_uuid = resolve_pipeline_identifier(ctx, workspace, repo_slug, pipeline_id).await?;

    let start = Instant::now();
    let is_table = ctx.renderer.format() == OutputFormat::Table;

    if is_table {
        eprintln!("Watching pipeline... (Ctrl-C to stop)");
    }

    loop {
        let pipeline = fetch_pipeline(ctx, workspace, repo_slug, &pipeline_uuid).await?;
        let status = get_pipeline_status(&pipeline);

        let steps = if show_steps {
            Some(fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid).await?)
        } else {
            None
        };

        if is_table {
            // Clear line and print status
            print!("\x1B[2K\r"); // Clear current line

            let build_num = pipeline
                .build_number
                .map(|n| format!("#{}", n))
                .unwrap_or_default();
            let ref_name = pipeline
                .target
                .as_ref()
                .and_then(|t| t.ref_name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let elapsed = format_elapsed(start);
            let icon = get_status_icon(&status);

            if let Some(ref step_list) = steps {
                let summary = format_steps_summary(step_list);
                print!(
                    "{} {} {} ({}) [{}] {}",
                    build_num, status, icon, ref_name, elapsed, summary
                );
            } else {
                print!(
                    "{} {} {} ({}) [{}]",
                    build_num, status, icon, ref_name, elapsed
                );
            }

            // Flush to show immediately
            use std::io::Write;
            std::io::stdout().flush().ok();
        }

        // Check if pipeline reached terminal state
        if is_terminal_state(&status) {
            if is_table {
                println!(); // New line after final status
                let icon = get_status_icon(&status);
                println!("\n{icon} Pipeline completed with status: {status}");
            } else {
                // For JSON/YAML/CSV: render final state once
                let steps_summary = steps
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .map(|s| format_steps_summary(s));
                let view = PipelineView {
                    uuid: pipeline.uuid,
                    build_number: pipeline
                        .build_number
                        .map(|n| n.to_string())
                        .unwrap_or_default(),
                    state: status,
                    ref_name: pipeline
                        .target
                        .as_ref()
                        .and_then(|t| t.ref_name.clone())
                        .unwrap_or_default(),
                    created: pipeline.created_on.unwrap_or_default(),
                    completed: pipeline.completed_on.unwrap_or_default(),
                    steps,
                    steps_summary,
                };
                ctx.renderer.render(&view)?;
            }
            break;
        }

        tokio::time::sleep(Duration::from_secs(interval)).await;
    }

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_icons() {
        assert_eq!(get_status_icon("SUCCESSFUL"), "✅");
        assert_eq!(get_status_icon("IN_PROGRESS"), "🔄");
        assert_eq!(get_status_icon("FAILED"), "❌");
        assert_eq!(get_status_icon("STOPPED"), "⏹");
        assert_eq!(get_status_icon("PENDING"), "⏳");
        assert_eq!(get_status_icon("UNKNOWN"), "❓");
    }

    #[test]
    fn test_terminal_states() {
        assert!(is_terminal_state("SUCCESSFUL"));
        assert!(is_terminal_state("FAILED"));
        assert!(is_terminal_state("STOPPED"));
        assert!(is_terminal_state("ERROR"));
        assert!(is_terminal_state("EXPIRED"));
        assert!(!is_terminal_state("IN_PROGRESS"));
        assert!(!is_terminal_state("PENDING"));
    }

    #[test]
    fn test_format_steps_summary() {
        let steps = vec![
            StepInfo {
                name: "Clone".to_string(),
                status: "SUCCESSFUL".to_string(),
            },
            StepInfo {
                name: "Build".to_string(),
                status: "IN_PROGRESS".to_string(),
            },
            StepInfo {
                name: "Deploy".to_string(),
                status: "PENDING".to_string(),
            },
        ];
        let summary = format_steps_summary(&steps);
        assert!(summary.contains("Clone ✅"));
        assert!(summary.contains("Build 🔄"));
        assert!(summary.contains("Deploy ⏳"));
    }

    #[test]
    fn test_format_elapsed() {
        // Can't easily test time-dependent function, but verify it compiles
        let start = Instant::now();
        let _elapsed = format_elapsed(start);
    }

    #[test]
    fn test_validate_sort_valid() {
        assert!(validate_sort_field("created_on").is_ok());
        assert!(validate_sort_field("-created_on").is_ok());
        assert!(validate_sort_field("updated_on").is_ok());
        assert!(validate_sort_field("-updated_on").is_ok());
        assert!(validate_sort_field("build_number").is_ok());
        assert!(validate_sort_field("-build_number").is_ok());
        assert!(validate_sort_field("state.name").is_ok());
        assert!(validate_sort_field("-state.name").is_ok());
    }

    #[test]
    fn test_validate_sort_invalid() {
        let result = validate_sort_field("invalid_field");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid sort field"));
        assert!(err.contains("invalid_field"));
    }

    #[test]
    fn test_build_request_path_initial() {
        let path = build_request_path(&None, "myworkspace", "myrepo", 100, "-created_on", None);
        assert!(path.contains("/2.0/repositories/myworkspace/myrepo/pipelines?"));
        assert!(path.contains("pagelen=100"));
        assert!(path.contains("sort=-created_on"));
    }

    #[test]
    fn test_build_request_path_with_branch() {
        let path = build_request_path(
            &None,
            "myworkspace",
            "myrepo",
            100,
            "-created_on",
            Some("main"),
        );
        // Should use q= filter syntax: q=target.ref_name%3D%22main%22
        assert!(path.contains("q=target.ref_name"));
        assert!(path.contains("%22main%22")); // URL-encoded quotes
    }

    #[test]
    fn test_build_request_path_next_page() {
        let next_url =
            Some("https://api.bitbucket.org/2.0/repositories/ws/repo/pipelines?page=2".to_string());
        let path = build_request_path(&next_url, "ws", "repo", 100, "-created_on", None);
        assert_eq!(path, "/2.0/repositories/ws/repo/pipelines?page=2");
    }

    #[test]
    fn test_steps_empty_returns_empty_summary() {
        let steps: Vec<StepInfo> = vec![];
        let summary = format_steps_summary(&steps);
        assert!(summary.is_empty());
    }
}
