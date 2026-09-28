//! `pipeline watch`.

use std::time::{Duration, Instant};

use anyhow::Result;
use atlassian_cli_output::OutputFormat;

use super::super::utils::BitbucketContext;
use super::list::{fetch_pipeline, resolve_pipeline_id};
use super::model::PipelineView;
use super::state::{
    format_elapsed, format_steps_summary, get_commit_hash, get_pipeline_status, get_status_icon,
    is_terminal_state,
};
use super::steps::fetch_steps;

/// Watch a pipeline until completion.
/// Returns the final status string for exit code handling.
#[allow(clippy::too_many_arguments)]
pub async fn watch_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_id: &str,
    interval: u64,
    show_steps: bool,
    on_complete: Option<&str>,
    timeout: Option<u64>,
    log_mode: bool,
) -> Result<String> {
    use std::io::IsTerminal;

    // Resolve build number to UUID if needed (only once at start)
    let pipeline_uuid = resolve_pipeline_id(ctx, workspace, repo_slug, pipeline_id).await?;

    let start = Instant::now();
    let is_table = matches!(
        ctx.renderer.format(),
        OutputFormat::Table | OutputFormat::Markdown
    );

    // Log mode: explicit --log flag OR table format piped to non-TTY
    let use_log_mode = log_mode || (is_table && !std::io::stdout().is_terminal());

    if is_table && !use_log_mode {
        eprintln!("Watching pipeline... (Ctrl-C to stop)");
    } else if use_log_mode {
        eprintln!("Watching pipeline in log mode... (Ctrl-C to stop)");
    }

    let final_status;

    loop {
        let pipeline = fetch_pipeline(ctx, workspace, repo_slug, &pipeline_uuid).await?;
        let status = get_pipeline_status(&pipeline);

        let steps = if show_steps {
            Some(fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid, false).await?)
        } else {
            None
        };

        // Render based on mode
        if is_table && !use_log_mode {
            // ANSI overwrite mode (interactive TTY)
            print!("\x1B[2K\r");

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
                let summary = format_steps_summary(step_list, true);
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

            use std::io::Write;
            std::io::stdout().flush().ok();
        } else if use_log_mode {
            // Log mode: one timestamped line per poll, no ANSI
            let now = chrono::Local::now().format("%H:%M:%S");
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
                let summary = format_steps_summary(step_list, false);
                println!(
                    "[{now}] {build_num} {status} {icon} ({ref_name}) [{elapsed}] [{summary}]"
                );
            } else {
                println!("[{now}] {build_num} {status} {icon} ({ref_name}) [{elapsed}]");
            }
        }
        // else: structured format (JSON/YAML/CSV) — no per-poll output

        // Check if pipeline reached terminal state
        if is_terminal_state(&status) {
            if is_table && !use_log_mode {
                println!();
                let icon = get_status_icon(&status);
                println!("\n{icon} Pipeline completed with status: {status}");
            } else if use_log_mode {
                let now = chrono::Local::now().format("%H:%M:%S");
                let icon = get_status_icon(&status);
                println!("[{now}] {icon} Pipeline completed: {status}");
            } else {
                // Structured output: render final state
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
                    state: status.clone(),
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

            // Run on-complete hook if specified
            if let Some(cmd) = on_complete {
                let build_number = pipeline
                    .build_number
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                let ref_name = pipeline
                    .target
                    .as_ref()
                    .and_then(|t| t.ref_name.clone())
                    .unwrap_or_default();
                tracing::info!(command = cmd, status = %status, "Running on-complete hook");
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .env("PIPELINE_STATUS", &status)
                    .env("PIPELINE_BUILD_NUMBER", &build_number)
                    .env("PIPELINE_UUID", &pipeline.uuid)
                    .env("PIPELINE_REF_NAME", &ref_name)
                    .output();
                match output {
                    Ok(o) if !o.status.success() => {
                        tracing::warn!(exit_code = ?o.status.code(), "on-complete hook failed");
                        eprintln!("Warning: on-complete hook exited with {}", o.status);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to run on-complete hook");
                        eprintln!("Warning: Failed to run on-complete hook: {e}");
                    }
                    _ => {}
                }
            }

            final_status = status;
            break;
        }

        // Check timeout before sleeping
        if let Some(timeout_secs) = timeout {
            if start.elapsed() > Duration::from_secs(timeout_secs) {
                tracing::warn!(timeout_secs, elapsed = ?start.elapsed(), "Watch timed out");

                if is_table || use_log_mode {
                    if is_table && !use_log_mode {
                        println!();
                    }
                    eprintln!("\nTimeout: pipeline did not complete within {timeout_secs}s");
                } else {
                    // Structured output on timeout: render current state
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
                        state: status.clone(),
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

                final_status = "TIMEOUT".to_string();
                break;
            }
        }

        tokio::time::sleep(Duration::from_secs(interval)).await;
    }

    Ok(final_status)
}
