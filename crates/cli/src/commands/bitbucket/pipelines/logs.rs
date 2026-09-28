//! `pipeline logs`.

use anyhow::{Context, Result};
use atlassian_cli_api::pagination::{fetch_paged, BitbucketPage, PageLimits};
use atlassian_cli_output::OutputFormat;

use super::super::utils::BitbucketContext;
use super::model::PipelineStep;
use super::state::get_step_status;

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
    let path = format!(
        "/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/?pagelen=100"
    );
    let (all_steps, page) =
        fetch_paged::<BitbucketPage<PipelineStep>>(&ctx.client, &path, PageLimits::new(None))
            .await
            .with_context(|| format!("Failed to fetch steps for pipeline {pipeline_uuid}"))?;

    if page.truncated {
        eprintln!(
            "warning: examined {} steps for pipeline {pipeline_uuid}; the list is incomplete.",
            all_steps.len()
        );
    }

    let mut steps_to_show: Vec<&PipelineStep> = all_steps.iter().collect();

    // Filter by step UUID if specified
    if let Some(uuid) = step_uuid {
        steps_to_show.retain(|s| s.uuid == uuid);
    }

    // Filter by step name pattern if specified
    if let Some(pattern) = step_name_pattern {
        steps_to_show.retain(|s| {
            s.name.as_ref().is_some_and(|n| {
                if ignore_case {
                    n.to_lowercase().contains(&pattern.to_lowercase())
                } else {
                    n.contains(pattern)
                }
            })
        });
    }

    // Filter by failed steps only if specified
    // By outcome: a failed step's `state.name` is `COMPLETED`, so testing it
    // matched nothing and `--failed-only` always printed "No steps matched".
    if failed_only {
        steps_to_show.retain(|s| {
            matches!(
                get_step_status(s).to_uppercase().as_str(),
                "FAILED" | "ERROR"
            )
        });
    }

    // Prose for people; an empty result for scripts, which could not parse
    // the sentence this used to print in every format.
    if steps_to_show.is_empty() {
        return ctx.renderer.render_list_or_empty(
            &Vec::<serde_json::Value>::new(),
            "No steps matched the filter criteria",
        );
    }

    // Prepare output for structured formats
    let mut all_logs = Vec::new();

    // Process each step
    for step in steps_to_show {
        let step_name = step.name.as_deref().unwrap_or("unnamed");
        let state_name = get_step_status(step);

        // Check if step was skipped
        if matches!(state_name.to_uppercase().as_str(), "NOT_RUN" | "SKIPPED") {
            if matches!(
                ctx.renderer.format(),
                OutputFormat::Table | OutputFormat::Markdown
            ) {
                println!("⏭  Step '{}' was skipped - no logs available", step_name);
            }
            continue;
        }

        // Fetch logs for this step
        let log_path = format!(
            "/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/{}/log",
            step.uuid
        );

        let log_content = match ctx.client.get_text(&log_path).await {
            Ok(content) => content,
            Err(e) => {
                // Build browser URL for fallback viewing
                let clean_pipeline_uuid = pipeline_uuid.trim_matches(|c| c == '{' || c == '}');
                let clean_step_uuid = step.uuid.trim_matches(|c| c == '{' || c == '}');
                let browser_url = format!(
                    "https://bitbucket.org/{}/{}/pipelines/results/{}/steps/{}",
                    workspace, repo_slug, clean_pipeline_uuid, clean_step_uuid
                );

                // Handle 404 as skipped step
                if e.to_string().contains("404") || e.to_string().contains("Not Found") {
                    if matches!(
                        ctx.renderer.format(),
                        OutputFormat::Table | OutputFormat::Markdown
                    ) {
                        println!("⏭  Step '{}' has no logs available yet", step_name);
                    }
                    continue;
                }

                return Err(anyhow::anyhow!(
                    "Failed to fetch logs for step '{}'.\n\nView in browser: {}\n\nError: {}",
                    step_name,
                    browser_url,
                    e
                ));
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

        // Output based on format
        match ctx.renderer.format() {
            OutputFormat::Table | OutputFormat::Quiet | OutputFormat::Markdown => {
                // Stream directly to stdout for table/quiet/markdown mode
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
        OutputFormat::Table | OutputFormat::Quiet | OutputFormat::Markdown
    ) {
        ctx.renderer.render_list(&all_logs)?;
    }

    Ok(())
}
