//! `pipeline status`: the latest pipeline as JSON, with an exit code scripts can branch on.

use std::time::Duration;

use anyhow::{Context, Result};

use super::super::utils::BitbucketContext;
use super::list::{build_request_path, fetch_pipeline, PipelineFilters};
use super::model::{PipelineList, PipelineStatusOutput};
use super::state::{get_commit_hash, get_pipeline_status, is_terminal_state, status_to_exit_code};
use super::steps::fetch_steps;

pub async fn pipeline_status(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    show_steps: bool,
    wait: bool,
    interval: u64,
) -> Result<()> {
    // Fetch single most recent pipeline to pin its UUID
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

    let initial_response: PipelineList =
        ctx.client.get(&path).await.with_context(|| {
            format!("Failed to fetch pipeline status for {workspace}/{repo_slug}")
        })?;

    if initial_response.values.is_empty() {
        tracing::info!(workspace, repo_slug, "No pipelines found");
        println!("{{}}");
        return Ok(());
    }

    // Pin the pipeline UUID so --wait doesn't drift to newer pipelines
    let pinned_uuid = initial_response.values[0].uuid.clone();

    loop {
        let pipeline = fetch_pipeline(ctx, workspace, repo_slug, &pinned_uuid).await?;
        let status = get_pipeline_status(&pipeline);

        if !wait || is_terminal_state(&status) {
            // Build status output
            let steps_data = if show_steps {
                fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid, true)
                    .await
                    .ok()
            } else {
                None
            };

            let status_output = PipelineStatusOutput {
                build_number: pipeline.build_number.unwrap_or(0),
                state: status.clone(),
                ref_name: pipeline
                    .target
                    .as_ref()
                    .and_then(|t| t.ref_name.clone())
                    .unwrap_or_default(),
                commit: get_commit_hash(&pipeline).unwrap_or_default(),
                created: pipeline.created_on.clone().unwrap_or_default(),
                steps: steps_data,
            };

            let json = serde_json::to_string_pretty(&status_output)?;
            println!("{}", json);

            let exit_code = status_to_exit_code(&status);
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
            return Ok(());
        }

        tracing::debug!(
            pipeline_uuid = %pinned_uuid,
            state = %status,
            interval,
            "Waiting for pipeline to complete"
        );
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}
