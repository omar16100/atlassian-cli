//! `pipeline watch`.

use std::time::{Duration, Instant};

use anyhow::Result;

use super::super::utils::BitbucketContext;
use super::list::{fetch_pipeline, resolve_pipeline_id};
use super::rows::{build_pipeline_view, pending_manual_steps};
use super::state::{
    format_elapsed, format_steps_summary, get_pipeline_status, get_status_icon, is_awaiting_action,
    is_finished, is_terminal_state, warn_if_unrecognised,
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
    let is_table = ctx.renderer.format().is_human();

    // Log mode: explicit --log flag OR table format piped to non-TTY
    let use_log_mode = log_mode || (is_table && !std::io::stdout().is_terminal());

    if is_table && !use_log_mode {
        eprintln!("Watching pipeline... (Ctrl-C to stop)");
    } else if use_log_mode {
        eprintln!("Watching pipeline in log mode... (Ctrl-C to stop)");
    }

    let final_status;
    let mut warned = false;

    loop {
        let pipeline = fetch_pipeline(ctx, workspace, repo_slug, &pipeline_uuid).await?;
        let status = get_pipeline_status(&pipeline);
        warn_if_unrecognised(&status, &mut warned);

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
            let now = local_clock();
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
        // else: structured format (JSON/YAML/CSV), no per-poll output

        // Check if pipeline reached terminal state, or stopped to wait for a
        // person: a paused build does not finish on its own, so it ends the
        // watch (exit code 3) instead of polling until --timeout.
        if is_terminal_state(&status) || is_finished(&pipeline) {
            let paused = is_awaiting_action(&status);
            let steps = if paused && steps.is_none() {
                fetch_steps(ctx, workspace, repo_slug, &pipeline.uuid, false)
                    .await
                    .map_err(
                        |e| tracing::warn!(error = %e, "Could not fetch steps of paused pipeline"),
                    )
                    .ok()
            } else {
                steps
            };
            let pending = pending_manual_steps(&status, steps.as_deref());
            tracing::info!(status = %status, pending_manual_steps = ?pending, "Watch finished");

            if is_table && !use_log_mode {
                println!();
                let icon = get_status_icon(&status);
                if paused {
                    println!("\n{icon} {}", paused_message(&status, pending));
                } else {
                    println!("\n{icon} Pipeline completed with status: {status}");
                }
            } else if use_log_mode {
                let now = local_clock();
                let icon = get_status_icon(&status);
                if paused {
                    println!("[{now}] {icon} {}", paused_message(&status, pending));
                } else {
                    println!("[{now}] {icon} Pipeline completed: {status}");
                }
            } else {
                // Structured output: render final state
                let view = build_pipeline_view(&pipeline, steps, show_steps, ctx.renderer.format());
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
                    let view =
                        build_pipeline_view(&pipeline, steps, show_steps, ctx.renderer.format());
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

/// The line `watch` ends on when the build is waiting for someone.
fn paused_message(status: &str, pending: Option<usize>) -> String {
    match pending {
        Some(1) => format!("Pipeline {status}: waiting on 1 manual step"),
        Some(n) if n > 0 => format!("Pipeline {status}: waiting on {n} manual steps"),
        _ => format!("Pipeline {status}: waiting on manual action"),
    }
}

/// The local wall clock time as `HH:MM:SS`, for the lines of the log mode.
fn local_clock() -> String {
    clock_text(&chrono::Local::now())
}

/// The wall clock time of `time` as `HH:MM:SS`, in the time zone of `time`.
fn clock_text<Tz: chrono::TimeZone>(time: &chrono::DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    time.format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The `pinned_` tests pin the clock text of the log mode. The expected
    // values were recorded with `chrono` and did not change when `chrono` was
    // removed. Only the helper `clock_at` knows the time types.

    /// `clock_text` for an RFC 3339 time, in the offset of that time.
    fn clock_at(time: &str) -> String {
        clock_text(&chrono::DateTime::parse_from_rfc3339(time).unwrap())
    }

    #[test]
    fn pinned_the_clock_text_is_hh_mm_ss_with_leading_zeros() {
        for (time, expected) in [
            ("2026-11-20T09:05:03+08:00", "09:05:03"),
            ("2026-11-20T00:00:00+00:00", "00:00:00"),
            ("2026-11-20T23:59:59.999999999-05:00", "23:59:59"),
            ("2026-11-20T12:34:56.5+05:30", "12:34:56"),
            ("2026-01-02T03:04:05Z", "03:04:05"),
        ] {
            assert_eq!(clock_at(time), expected, "time {time:?}");
        }
    }

    fn assert_clock_shape(clock: &str) {
        let bytes = clock.as_bytes();
        assert_eq!(bytes.len(), 8, "clock {clock:?}");
        assert_eq!(bytes[2], b':', "clock {clock:?}");
        assert_eq!(bytes[5], b':', "clock {clock:?}");
        let part = |from: usize| clock[from..from + 2].parse::<u32>().unwrap();
        assert!(bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 2 || i == 5 || b.is_ascii_digit()));
        assert!(part(0) < 24, "clock {clock:?}");
        assert!(part(3) < 60, "clock {clock:?}");
        assert!(part(6) < 60, "clock {clock:?}");
    }

    #[test]
    fn pinned_the_local_clock_has_the_hh_mm_ss_shape() {
        assert_clock_shape(&local_clock());
    }

    /// The local clock must agree with the system `date` command. The test
    /// runs on a multi-thread runtime, as the command does.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pinned_the_local_clock_agrees_with_the_date_command() {
        fn date_command() -> String {
            let output = std::process::Command::new("date")
                .arg("+%H:%M:%S")
                .output()
                .expect("the date command must run");
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        }

        // The clock can pass midnight between the three reads. Then the order
        // does not hold, so read again.
        for _ in 0..3 {
            let before = date_command();
            let clock = tokio::spawn(async { local_clock() }).await.unwrap();
            let after = date_command();
            assert_clock_shape(&clock);
            if before <= after {
                assert!(
                    before <= clock && clock <= after,
                    "local clock {clock} is not between {before} and {after}"
                );
                return;
            }
        }
        panic!("the clock passed midnight three times");
    }

    #[test]
    fn the_paused_message_counts_manual_steps() {
        assert_eq!(
            paused_message("PAUSED", Some(2)),
            "Pipeline PAUSED: waiting on 2 manual steps"
        );
        assert_eq!(
            paused_message("PAUSED", Some(1)),
            "Pipeline PAUSED: waiting on 1 manual step"
        );
        assert_eq!(
            paused_message("HALTED", None),
            "Pipeline HALTED: waiting on manual action"
        );
    }
}
