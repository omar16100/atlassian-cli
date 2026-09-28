//! `pipeline list`, `get` and `latest`, plus pipeline lookup shared by the other commands.

use anyhow::{Context, Result};
use atlassian_cli_output::OutputFormat;
use url::{self, form_urlencoded};

use super::super::utils::{accept_safe_identifier, BitbucketContext};
use super::model::{Pipeline, PipelineList, PipelineRow, PipelineView};
use super::state::{
    format_status_for_display, format_steps_summary, get_commit_hash, get_pipeline_status,
};
use super::steps::fetch_steps;
use crate::query::FilterBuilder;

// Valid sort fields for pipeline list
pub(super) const VALID_SORT_FIELDS: &[&str] = &[
    "created_on",
    "-created_on",
    "updated_on",
    "-updated_on",
    "build_number",
    "-build_number",
    "state.name",
    "-state.name",
];

pub(super) fn validate_sort_field(sort: &str) -> Result<()> {
    if !VALID_SORT_FIELDS.contains(&sort) {
        anyhow::bail!(
            "Invalid sort field '{}'. Valid options: {}",
            sort,
            VALID_SORT_FIELDS.join(", ")
        );
    }
    Ok(())
}

pub(super) struct PipelineFilters<'a> {
    pub(super) branch: Option<&'a str>,
    pub(super) since: Option<&'a str>,
    pub(super) before: Option<&'a str>,
}

pub(super) fn build_request_path(
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
        // The single gate for a user-supplied pipeline identifier. Its result is
        // interpolated into six different paths, so validating here is both the
        // one place that covers them all and the one place that cannot be
        // forgotten when a seventh is added.
        accept_safe_identifier(identifier, "pipeline identifier")?;
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

pub(super) async fn fetch_pipeline(
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
    let use_colors = matches!(
        ctx.renderer.format(),
        OutputFormat::Table | OutputFormat::Markdown
    );
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
        .zip(step_summaries)
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
    }

    tracing::debug!(workspace, repo_slug, count = rows.len(), "Listed pipelines");

    ctx.renderer
        .render_list_or_empty(&rows, "No pipelines found")
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
    let use_colors = matches!(
        ctx.renderer.format(),
        OutputFormat::Table | OutputFormat::Markdown
    );
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
