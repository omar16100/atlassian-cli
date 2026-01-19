use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use atlassian_cli_output::OutputFormat;
use serde::{Deserialize, Serialize};
use url::{self, form_urlencoded};

use super::utils::BitbucketContext;
use crate::commands::common::{render_success, MutationResult};
use crate::query::FilterBuilder;

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
    #[serde(default)]
    commit: Option<CommitInfo>,
}

#[derive(Deserialize, Clone)]
struct CommitInfo {
    #[serde(default)]
    hash: Option<String>,
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
    #[serde(default)]
    started_on: Option<String>,
    #[serde(default)]
    completed_on: Option<String>,
    #[serde(default)]
    duration_in_seconds: Option<u64>,
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
    commit: String,
    target_type: String,
    created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps_summary: Option<String>,
}

#[derive(Serialize)]
struct PipelineView {
    uuid: String,
    build_number: String,
    state: String,
    ref_name: String,
    commit: String,
    created: String,
    completed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps: Option<Vec<StepInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps_summary: Option<String>,
}

#[derive(Serialize, Clone)]
struct StepInfo {
    uuid: String,
    name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    started: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logs_url: Option<String>,
}

#[derive(Serialize)]
struct PipelineStatusOutput {
    build_number: i64,
    state: String,
    ref_name: String,
    commit: String,
    created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps: Option<Vec<StepInfo>>,
}

#[derive(Serialize, Clone, Debug)]
struct PipelineVariable {
    key: String,
    value: String,
    secured: bool,
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

fn format_status_for_display(status: &str, use_colors: bool) -> String {
    use atlassian_cli_output::StatusFormatter;

    let icon = get_status_icon(status);
    if use_colors {
        let formatter = StatusFormatter::new();
        formatter.format(status, icon)
    } else {
        format!("{} {}", status, icon)
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

fn get_commit_hash(pipeline: &Pipeline) -> String {
    pipeline
        .target
        .as_ref()
        .and_then(|t| t.commit.as_ref())
        .and_then(|c| c.hash.as_ref())
        .map(|h| h.chars().take(7).collect::<String>())
        .unwrap_or_default()
}

fn is_terminal_state(status: &str) -> bool {
    matches!(
        status.to_uppercase().as_str(),
        "SUCCESSFUL" | "FAILED" | "STOPPED" | "ERROR" | "EXPIRED" | "COMPLETED"
    )
}

fn format_steps_summary(steps: &[StepInfo], use_colors: bool) -> String {
    use atlassian_cli_output::StatusFormatter;
    let formatter = if use_colors {
        StatusFormatter::new()
    } else {
        StatusFormatter::with_colors(false)
    };

    steps
        .iter()
        .map(|s| {
            let icon = get_status_icon(&s.status);
            if use_colors {
                format!("{} {}", s.name, formatter.format(&s.status, icon))
            } else {
                format!("{} {} {}", s.name, s.status, icon)
            }
        })
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

fn format_duration_secs(seconds: u64) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    let hours = mins / 60;
    if hours > 0 {
        format!("{}h {}m {}s", hours, mins % 60, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
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

fn parse_variables(vars: Vec<String>, secured: bool) -> Result<Vec<PipelineVariable>> {
    let mut variables = Vec::new();

    for var_str in vars {
        let parts: Vec<&str> = var_str.splitn(2, '=').collect();

        if parts.len() != 2 {
            anyhow::bail!("Invalid variable format '{}'. Expected KEY=VALUE", var_str);
        }

        let key = parts[0].trim();
        let value = parts[1]; // Don't trim value - preserve whitespace

        if key.is_empty() {
            anyhow::bail!("Variable key cannot be empty in '{}'", var_str);
        }

        variables.push(PipelineVariable {
            key: key.to_string(),
            value: value.to_string(),
            secured,
        });
    }

    Ok(variables)
}

struct PipelineFilters<'a> {
    branch: Option<&'a str>,
    since: Option<&'a str>,
    before: Option<&'a str>,
}

fn build_request_path(
    next_url: &Option<String>,
    workspace: &str,
    repo_slug: &str,
    page_size: usize,
    sort: &str,
    filters: PipelineFilters,
) -> String {
    if let Some(url_str) = next_url {
        // Validate server-provided URL to prevent SSRF attacks
        if let Ok(parsed_url) = url::Url::parse(url_str) {
            // Only accept HTTPS URLs from api.bitbucket.org
            if parsed_url.scheme() == "https" && parsed_url.host_str() == Some("api.bitbucket.org")
            {
                return parsed_url.path().to_string()
                    + parsed_url
                        .query()
                        .map(|q| format!("?{}", q))
                        .unwrap_or_default()
                        .as_str();
            }
        }
        // If validation fails, fall back to building the URL manually
        tracing::warn!("Invalid or untrusted pagination URL from server, building manually");
    }

    {
        let mut query = form_urlencoded::Serializer::new(String::new());
        query.append_pair("pagelen", &page_size.to_string());
        query.append_pair("sort", sort);

        // Build combined filters with AND logic using FilterBuilder
        let mut filter_builder = FilterBuilder::new();

        if let Some(b) = filters.branch.filter(|s| !s.is_empty()) {
            filter_builder = filter_builder.add_eq("target.ref_name", b);
        }

        if let Some(s) = filters.since {
            filter_builder = filter_builder.add_gte("created_on", s);
        }

        if let Some(b) = filters.before {
            filter_builder = filter_builder.add_lt("created_on", b);
        }

        // Add the filter query parameter if any filters were added
        let filter_query = filter_builder.finish();
        if !filter_query.is_empty() {
            query.append_pair("q", &filter_query);
        }

        format!(
            "/2.0/repositories/{workspace}/{repo_slug}/pipelines?{}",
            query.finish()
        )
    }
}

/// Resolve pipeline identifier: build number (e.g. "404") -> UUID
pub async fn resolve_pipeline_id(
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
    include_details: bool,
) -> Result<Vec<StepInfo>> {
    let path =
        format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/");
    let response: StepList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch steps for pipeline {pipeline_uuid}"))?;

    let clean_pipeline_uuid = pipeline_uuid.trim_matches('{').trim_matches('}');
    Ok(response
        .values
        .iter()
        .map(|step| {
            let clean_step_uuid = step.uuid.trim_matches('{').trim_matches('}');
            StepInfo {
                uuid: step.uuid.clone(),
                name: step.name.clone().unwrap_or_else(|| step.uuid.clone()),
                status: get_step_status(step),
                started: if include_details {
                    step.started_on.clone()
                } else {
                    None
                },
                completed: if include_details {
                    step.completed_on.clone()
                } else {
                    None
                },
                duration: if include_details {
                    step.duration_in_seconds.map(format_duration_secs)
                } else {
                    None
                },
                logs_url: if include_details {
                    Some(format!(
                        "https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{clean_pipeline_uuid}/steps/{clean_step_uuid}"
                    ))
                } else {
                    None
                },
            }
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
    since: Option<&str>,
    before: Option<&str>,
    fetch_all: bool,
    show_steps: bool,
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
            PipelineFilters {
                branch,
                since,
                before,
            },
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

    // Fetch steps for each pipeline if requested
    let use_colors = ctx.renderer.format() == OutputFormat::Table;
    let step_summaries: Vec<Option<String>> = if show_steps {
        let mut summaries = Vec::with_capacity(all_pipelines.len());
        for pipeline in &all_pipelines {
            let steps = fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid, false).await;
            let summary = steps
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| format_steps_summary(&s, use_colors));
            summaries.push(summary);
        }
        summaries
    } else {
        vec![None; all_pipelines.len()]
    };
    let rows: Vec<PipelineRow> = all_pipelines
        .iter()
        .zip(step_summaries.into_iter())
        .map(|(pipeline, steps_summary)| {
            let status = get_pipeline_status(pipeline);
            PipelineRow {
                build_number: pipeline
                    .build_number
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
                state: format_status_for_display(&status, use_colors),
                ref_name: pipeline
                    .target
                    .as_ref()
                    .and_then(|t| t.ref_name.clone())
                    .unwrap_or_default(),
                commit: get_commit_hash(pipeline),
                target_type: pipeline
                    .target
                    .as_ref()
                    .and_then(|t| t.target_type.clone())
                    .unwrap_or_default(),
                created: pipeline.created_on.clone().unwrap_or_default(),
                steps_summary,
            }
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No pipelines found");
        println!("No pipelines found");
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
    let pipeline_uuid = resolve_pipeline_id(ctx, workspace, repo_slug, pipeline_id).await?;
    let pipeline = fetch_pipeline(ctx, workspace, repo_slug, &pipeline_uuid).await?;

    let steps = if show_steps {
        Some(fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid, true).await?)
    } else {
        None
    };

    // Only include steps_summary if steps is non-empty
    let use_colors = ctx.renderer.format() == OutputFormat::Table;
    let steps_summary = steps
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| format_steps_summary(s, use_colors));
    let status = get_pipeline_status(&pipeline);
    let state = format_status_for_display(&status, use_colors);

    let view = PipelineView {
        uuid: pipeline.uuid.clone(),
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
        commit: get_commit_hash(&pipeline),
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
    variable_strings: Vec<String>,
    secured: bool,
) -> Result<()> {
    let mut payload = serde_json::json!({
        "target": {
            "ref_name": ref_name,
            "ref_type": ref_type,
            "type": "pipeline_ref_target"
        }
    });

    // Add variables if provided
    if !variable_strings.is_empty() {
        let variables = parse_variables(variable_strings.clone(), secured)?;
        payload["variables"] = serde_json::to_value(variables)?;
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/");
    let pipeline: Pipeline = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to trigger pipeline for {ref_name} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        build_number = pipeline.build_number,
        ref_name,
        workspace,
        repo_slug,
        variables_count = variable_strings.len(),
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

    render_success(
        ctx.renderer,
        &format!("✅ Pipeline {pipeline_uuid} stopped on {workspace}/{repo_slug}"),
        &MutationResult::with_id(
            format!("Pipeline stopped on {workspace}/{repo_slug}"),
            pipeline_uuid,
        ),
    )
}

#[allow(clippy::too_many_arguments)]
pub async fn get_pipeline_logs(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
    step_uuid: Option<&str>,
    step_name_pattern: Option<&str>,
    grep_pattern: Option<&str>,
    ignore_case: bool,
    failed_only: bool,
) -> Result<()> {
    tracing::info!(
        pipeline_uuid,
        workspace,
        repo_slug,
        "Fetching pipeline logs with filters"
    );

    // Fetch all pipeline steps
    let path =
        format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/");
    let response: StepList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch steps for pipeline {pipeline_uuid}"))?;

    let mut steps_to_show: Vec<&PipelineStep> = response.values.iter().collect();

    // Filter by step UUID if specified
    if let Some(uuid) = step_uuid {
        steps_to_show.retain(|s| s.uuid == uuid);
    }

    // Filter by step name pattern if specified
    if let Some(pattern) = step_name_pattern {
        steps_to_show.retain(|s| s.name.as_ref().is_some_and(|n| n.contains(pattern)));
    }

    // Filter by failed steps only if specified
    if failed_only {
        steps_to_show.retain(|s| {
            s.state.as_ref().is_some_and(|state| {
                matches!(state.name.to_uppercase().as_str(), "FAILED" | "ERROR")
            })
        });
    }

    if steps_to_show.is_empty() {
        println!("No steps matched the filter criteria");
        return Ok(());
    }

    // Prepare output for structured formats
    let mut all_logs = Vec::new();

    // Process each step
    for step in steps_to_show {
        let step_name = step.name.as_deref().unwrap_or("unnamed");
        let step_state = step.state.as_ref();

        // Check if step was skipped
        if let Some(state) = step_state {
            if matches!(state.name.to_uppercase().as_str(), "NOT_RUN" | "SKIPPED") {
                if ctx.renderer.format() == OutputFormat::Table {
                    println!("⏭  Step '{}' was skipped - no logs available", step_name);
                }
                continue;
            }
        }

        // Fetch logs for this step
        let log_path = format!(
            "/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/{}/log",
            step.uuid
        );

        let log_content = match ctx.client.get_text(&log_path).await {
            Ok(content) => content,
            Err(e) => {
                // Handle 404 as skipped step
                if e.to_string().contains("404") || e.to_string().contains("Not Found") {
                    if ctx.renderer.format() == OutputFormat::Table {
                        println!("⏭  Step '{}' has no logs available yet", step_name);
                    }
                    continue;
                }
                return Err(e).with_context(|| {
                    format!(
                        "Failed to fetch logs for step {} ({})",
                        step_name, step.uuid
                    )
                });
            }
        };

        // Apply grep filter if specified
        let filtered_lines: Vec<&str> = if let Some(pattern) = grep_pattern {
            log_content
                .lines()
                .filter(|line| {
                    if ignore_case {
                        line.to_lowercase().contains(&pattern.to_lowercase())
                    } else {
                        line.contains(pattern)
                    }
                })
                .collect()
        } else {
            log_content.lines().collect()
        };

        let state_name = step_state.map(|s| s.name.as_str()).unwrap_or("UNKNOWN");

        // Output based on format
        match ctx.renderer.format() {
            OutputFormat::Table | OutputFormat::Quiet => {
                // Stream directly to stdout for table/quiet mode
                println!("\n=== Step: {} ({}) ===", step_name, state_name);
                for line in filtered_lines {
                    println!("{}", line);
                }
            }
            _ => {
                // Collect for structured output
                all_logs.push(serde_json::json!({
                    "step_uuid": step.uuid,
                    "step_name": step_name,
                    "step_status": state_name,
                    "log_lines": filtered_lines,
                    "filtered_count": if grep_pattern.is_some() {
                        Some(filtered_lines.len())
                    } else {
                        None
                    },
                    "total_lines": log_content.lines().count(),
                }));
            }
        }
    }

    // Render structured output for JSON/YAML/CSV
    if !matches!(
        ctx.renderer.format(),
        OutputFormat::Table | OutputFormat::Quiet
    ) {
        ctx.renderer.render(&all_logs)?;
    }

    Ok(())
}

pub async fn list_steps(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_id: &str,
) -> Result<()> {
    // Resolve build number to UUID if needed
    let pipeline_uuid = resolve_pipeline_id(ctx, workspace, repo_slug, pipeline_id).await?;

    tracing::debug!(
        pipeline_uuid,
        workspace,
        repo_slug,
        "Listing pipeline steps"
    );

    let steps = fetch_steps(ctx, workspace, repo_slug, &pipeline_uuid, true).await?;

    if steps.is_empty() {
        tracing::info!(pipeline_uuid, "No steps found");
        println!("No steps found");
        return Ok(());
    }

    tracing::debug!(pipeline_uuid, count = steps.len(), "Listed pipeline steps");

    ctx.renderer.render(&steps)
}

pub async fn pipeline_status(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    show_steps: bool,
) -> Result<()> {
    // Fetch single most recent pipeline
    let path = build_request_path(
        &None,
        workspace,
        repo_slug,
        1,
        "-created_on",
        PipelineFilters {
            branch: None,
            since: None,
            before: None,
        },
    );

    let response: PipelineList =
        ctx.client.get(&path).await.with_context(|| {
            format!("Failed to fetch pipeline status for {workspace}/{repo_slug}")
        })?;

    if response.values.is_empty() {
        tracing::info!(workspace, repo_slug, "No pipelines found");
        // Output empty JSON and return success
        println!("{{}}");
        return Ok(());
    }

    let pipeline = &response.values[0];

    // Build status output
    let steps_data = if show_steps {
        fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid, true)
            .await
            .ok()
    } else {
        None
    };

    let status = get_pipeline_status(pipeline);
    let status_output = PipelineStatusOutput {
        build_number: pipeline.build_number.unwrap_or(0),
        state: status.clone(),
        ref_name: pipeline
            .target
            .as_ref()
            .and_then(|t| t.ref_name.clone())
            .unwrap_or_default(),
        commit: get_commit_hash(pipeline),
        created: pipeline.created_on.clone().unwrap_or_default(),
        steps: steps_data,
    };

    // Force JSON output regardless of --output flag
    let json = serde_json::to_string_pretty(&status_output)?;
    println!("{}", json);

    // Determine exit code based on state
    let exit_code = match status.to_uppercase().as_str() {
        "SUCCESSFUL" | "COMPLETED" => 0,
        "FAILED" | "ERROR" | "STOPPED" | "EXPIRED" => 1,
        "PENDING" | "IN_PROGRESS" => 2,
        _ => 0,
    };

    tracing::debug!(
        workspace,
        repo_slug,
        state = %status,
        exit_code,
        "Pipeline status"
    );

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

pub async fn rerun_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_id: &str,
    variable_strings: Vec<String>,
    secured: bool,
) -> Result<()> {
    // Resolve build number to UUID if needed
    let pipeline_uuid = resolve_pipeline_id(ctx, workspace, repo_slug, pipeline_id).await?;

    // Fetch original pipeline
    let original = fetch_pipeline(ctx, workspace, repo_slug, &pipeline_uuid).await?;

    tracing::info!(pipeline_id, workspace, repo_slug, "Re-running pipeline");

    // Extract commit hash or fallback to ref_name
    let mut payload = if let Some(commit_hash) = original
        .target
        .as_ref()
        .and_then(|t| t.commit.as_ref())
        .and_then(|c| c.hash.as_ref())
    {
        // Trigger with commit target
        serde_json::json!({
            "target": {
                "commit": {
                    "type": "commit",
                    "hash": commit_hash
                },
                "type": "pipeline_commit_target"
            }
        })
    } else {
        // Fallback to ref_name if no commit info
        let ref_name = original
            .target
            .as_ref()
            .and_then(|t| t.ref_name.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Pipeline has no commit or ref info"))?;

        let ref_type = original
            .target
            .as_ref()
            .and_then(|t| t.target_type.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("branch");

        serde_json::json!({
            "target": {
                "ref_name": ref_name,
                "ref_type": ref_type,
                "type": "pipeline_ref_target"
            }
        })
    };

    // Add variables if provided
    if !variable_strings.is_empty() {
        let variables = parse_variables(variable_strings.clone(), secured)?;
        payload["variables"] = serde_json::to_value(variables)?;
    }

    // Trigger new pipeline
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/");
    let new_pipeline: Pipeline = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to re-run pipeline {pipeline_id} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        original_id = pipeline_id,
        new_build_number = new_pipeline.build_number,
        variables_count = variable_strings.len(),
        "Pipeline re-run triggered"
    );

    // Return same format as trigger_pipeline
    #[derive(Serialize)]
    struct Triggered {
        uuid: String,
        build_number: String,
        state: String,
        ref_name: String,
        rerun_from: String,
    }

    let state = get_pipeline_status(&new_pipeline);
    let triggered = Triggered {
        uuid: new_pipeline.uuid,
        build_number: new_pipeline
            .build_number
            .map(|n| n.to_string())
            .unwrap_or_default(),
        state,
        ref_name: new_pipeline
            .target
            .as_ref()
            .and_then(|t| t.ref_name.clone())
            .unwrap_or_default(),
        rerun_from: pipeline_id.to_string(),
    };

    ctx.renderer.render(&triggered)
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
    let pipeline_uuid = resolve_pipeline_id(ctx, workspace, repo_slug, pipeline_id).await?;

    let start = Instant::now();
    let is_table = ctx.renderer.format() == OutputFormat::Table;

    if is_table {
        eprintln!("Watching pipeline... (Ctrl-C to stop)");
    }

    loop {
        let pipeline = fetch_pipeline(ctx, workspace, repo_slug, &pipeline_uuid).await?;
        let status = get_pipeline_status(&pipeline);

        let steps = if show_steps {
            Some(fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid, false).await?)
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
                // Always use colors for watch command in table mode
                let summary = format_steps_summary(step_list, is_table);
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
                // For JSON/YAML/CSV: render final state once (no colors)
                let steps_summary = steps
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .map(|s| format_steps_summary(s, false));
                let view = PipelineView {
                    uuid: pipeline.uuid.clone(),
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
                    commit: get_commit_hash(&pipeline),
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

/// Find the latest pipeline for a given branch
/// Returns the pipeline UUID
pub async fn find_latest_pipeline_for_branch(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    branch: &str,
) -> Result<String> {
    let path = build_request_path(
        &None,
        workspace,
        repo_slug,
        1,             // limit
        "-created_on", // sort
        PipelineFilters {
            branch: Some(branch),
            since: None,
            before: None,
        },
    );

    let response: PipelineList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch pipelines for branch {branch}"))?;

    let pipeline = response
        .values
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No pipelines found for branch {}", branch))?;

    Ok(pipeline.uuid)
}

/// Check if a pipeline has failed steps
pub async fn pipeline_has_failed_steps(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
) -> Result<bool> {
    let path =
        format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/");
    let response: StepList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch steps for pipeline {pipeline_uuid}"))?;

    Ok(response.values.iter().any(|step| {
        step.state
            .as_ref()
            .is_some_and(|state| matches!(state.name.to_uppercase().as_str(), "FAILED" | "ERROR"))
    }))
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
                uuid: "{uuid1}".to_string(),
                name: "Clone".to_string(),
                status: "SUCCESSFUL".to_string(),
                started: None,
                completed: None,
                duration: None,
                logs_url: None,
            },
            StepInfo {
                uuid: "{uuid2}".to_string(),
                name: "Build".to_string(),
                status: "IN_PROGRESS".to_string(),
                started: None,
                completed: None,
                duration: None,
                logs_url: None,
            },
            StepInfo {
                uuid: "{uuid3}".to_string(),
                name: "Deploy".to_string(),
                status: "PENDING".to_string(),
                started: None,
                completed: None,
                duration: None,
                logs_url: None,
            },
        ];
        let summary = format_steps_summary(&steps, false);
        assert!(summary.contains("Clone"));
        assert!(summary.contains("✅"));
        assert!(summary.contains("Build"));
        assert!(summary.contains("🔄"));
        assert!(summary.contains("Deploy"));
        assert!(summary.contains("⏳"));
    }

    #[test]
    fn test_format_elapsed() {
        // Can't easily test time-dependent function, but verify it compiles
        let start = Instant::now();
        let _elapsed = format_elapsed(start);
    }

    #[test]
    fn test_format_duration_secs() {
        assert_eq!(format_duration_secs(0), "0s");
        assert_eq!(format_duration_secs(45), "45s");
        assert_eq!(format_duration_secs(60), "1m 0s");
        assert_eq!(format_duration_secs(90), "1m 30s");
        assert_eq!(format_duration_secs(3600), "1h 0m 0s");
        assert_eq!(format_duration_secs(3661), "1h 1m 1s");
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
        let path = build_request_path(
            &None,
            "myworkspace",
            "myrepo",
            100,
            "-created_on",
            PipelineFilters {
                branch: None,
                since: None,
                before: None,
            },
        );
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
            PipelineFilters {
                branch: Some("main"),
                since: None,
                before: None,
            },
        );
        // Should use q= filter syntax: q=target.ref_name%3D%22main%22
        assert!(path.contains("q=target.ref_name"));
        assert!(path.contains("%22main%22")); // URL-encoded quotes
    }

    #[test]
    fn test_build_request_path_next_page() {
        let next_url =
            Some("https://api.bitbucket.org/2.0/repositories/ws/repo/pipelines?page=2".to_string());
        let path = build_request_path(
            &next_url,
            "ws",
            "repo",
            100,
            "-created_on",
            PipelineFilters {
                branch: None,
                since: None,
                before: None,
            },
        );
        assert_eq!(path, "/2.0/repositories/ws/repo/pipelines?page=2");
    }

    #[test]
    fn test_build_request_path_with_time_filters() {
        let path = build_request_path(
            &None,
            "myworkspace",
            "myrepo",
            100,
            "-created_on",
            PipelineFilters {
                branch: None,
                since: Some("2024-01-01T00:00:00Z"),
                before: Some("2024-12-31T23:59:59Z"),
            },
        );
        // Should contain both time filters with AND logic and parentheses
        assert!(path.contains("created_on"));
        assert!(path.contains("%3E%3D")); // URL-encoded >=
        assert!(path.contains("%3C")); // URL-encoded <
        assert!(path.contains("2024-01-01"));
        assert!(path.contains("2024-12-31"));
        assert!(path.contains("AND"));
    }

    #[test]
    fn test_build_request_path_with_branch_and_time() {
        let path = build_request_path(
            &None,
            "myworkspace",
            "myrepo",
            100,
            "-created_on",
            PipelineFilters {
                branch: Some("main"),
                since: Some("2024-01-01T00:00:00Z"),
                before: None,
            },
        );
        // Should combine branch and time filters with parentheses
        assert!(path.contains("target.ref_name"));
        assert!(path.contains("created_on"));
        assert!(path.contains("AND"));
        assert!(path.contains("%28")); // URL-encoded (
        assert!(path.contains("%29")); // URL-encoded )
    }

    #[test]
    fn test_steps_empty_returns_empty_summary() {
        let steps: Vec<StepInfo> = vec![];
        let summary = format_steps_summary(&steps, false);
        assert!(summary.is_empty());
    }

    #[test]
    fn test_parse_variables_valid() {
        let vars = vec!["ENV=prod".to_string(), "DEBUG=true".to_string()];
        let result = parse_variables(vars, false).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "ENV");
        assert_eq!(result[0].value, "prod");
        assert!(!result[0].secured);
    }

    #[test]
    fn test_parse_variables_with_equals_in_value() {
        let vars = vec!["URL=http://example.com?foo=bar".to_string()];
        let result = parse_variables(vars, false).unwrap();
        assert_eq!(result[0].value, "http://example.com?foo=bar");
    }

    #[test]
    fn test_parse_variables_secured() {
        let vars = vec!["SECRET=value".to_string()];
        let result = parse_variables(vars, true).unwrap();
        assert!(result[0].secured);
    }

    #[test]
    fn test_parse_variables_invalid_format() {
        let vars = vec!["NOEQUALS".to_string()];
        let result = parse_variables(vars, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Expected KEY=VALUE"));
    }

    #[test]
    fn test_parse_variables_empty_key() {
        let vars = vec!["=value".to_string()];
        let result = parse_variables(vars, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("key cannot be empty"));
    }

    #[test]
    fn test_parse_variables_preserves_whitespace_in_value() {
        let vars = vec!["MSG= spaces  everywhere ".to_string()];
        let result = parse_variables(vars, false).unwrap();
        assert_eq!(result[0].value, " spaces  everywhere ");
    }

    #[test]
    fn test_target_with_commit_deserializes() {
        let json = r#"{
            "ref_name": "main",
            "type": "pipeline_ref_target",
            "commit": {"hash": "abc123"}
        }"#;
        let target: Target = serde_json::from_str(json).unwrap();
        assert_eq!(target.commit.unwrap().hash.unwrap(), "abc123");
    }

    #[test]
    fn test_target_without_commit_deserializes() {
        let json = r#"{
            "ref_name": "main",
            "type": "pipeline_ref_target"
        }"#;
        let target: Target = serde_json::from_str(json).unwrap();
        assert!(target.commit.is_none());
    }
}
