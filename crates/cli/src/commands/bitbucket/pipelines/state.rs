//! Pipeline and step state: status derivation, icons, exit codes, summaries.

use std::time::Instant;

use super::model::{Pipeline, PipelineStep, StepInfo};

// ============================================================================
// Helper Functions
// ============================================================================

pub(super) fn get_status_icon(status: &str) -> &'static str {
    match status.to_uppercase().as_str() {
        "SUCCESSFUL" | "COMPLETED" => "✅",
        "IN_PROGRESS" | "RUNNING" => "🔄",
        "FAILED" | "ERROR" => "❌",
        "STOPPED" => "⏹",
        "PENDING" | "NOT_RUN" | "READY" => "⏳",
        "PAUSED" | "HALTED" => "⏸",
        _ => "❓",
    }
}

pub(super) fn format_status_for_display(status: &str, use_colors: bool) -> String {
    use atlassian_cli_output::StatusFormatter;

    let icon = get_status_icon(status);
    if use_colors {
        let formatter = StatusFormatter::new();
        formatter.format(status, icon)
    } else {
        format!("{} {}", status, icon)
    }
}

/// A step's outcome: `state.result.name` once it has finished, else `state.name`.
///
/// A finished step always has `state.name == "COMPLETED"`; whether it passed,
/// failed or never ran is only in `result`. Reading `state.name` is how
/// `logs --failed-only` came to match nothing.
pub(super) fn get_step_status(step: &PipelineStep) -> String {
    step.state
        .as_ref()
        .and_then(|s| s.result.as_ref().map(|r| r.name.clone()))
        .or_else(|| step.state.as_ref().map(|s| s.name.clone()))
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

/// The status to report for a pipeline.
///
/// `result.name` once it has finished. While `IN_PROGRESS`, the stage when it
/// is anything but `RUNNING`: Bitbucket reports a build waiting on a manual
/// step as `IN_PROGRESS` with stage `PAUSED`, and showing `IN_PROGRESS` there
/// left a paused build indistinguishable from a running one for days.
pub(super) fn get_pipeline_status(pipeline: &Pipeline) -> String {
    let Some(state) = pipeline.state.as_ref() else {
        return "UNKNOWN".to_string();
    };
    if let Some(result) = &state.result {
        return result.name.clone();
    }
    if state.name.eq_ignore_ascii_case("IN_PROGRESS") {
        if let Some(stage) = state.stage.as_ref() {
            if !stage.name.is_empty() && !stage.name.eq_ignore_ascii_case("RUNNING") {
                return stage.name.clone();
            }
        }
    }
    state.name.clone()
}

/// A build that will not move until someone acts on it.
pub(super) fn is_awaiting_action(status: &str) -> bool {
    matches!(status.to_uppercase().as_str(), "PAUSED" | "HALTED")
}

/// Bitbucket's step trigger type without its `pipeline_step_trigger_` prefix,
/// upper-cased: `MANUAL` or `AUTOMATIC`. Values without the prefix pass
/// through upper-cased, so an unexpected type is still shown rather than lost.
pub(super) fn normalize_trigger(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    lower
        .strip_prefix("pipeline_step_trigger_")
        .unwrap_or(&lower)
        .to_ascii_uppercase()
}

/// Manual steps that have not been started: what a paused build waits on.
pub(super) fn count_pending_manual_steps(steps: &[StepInfo]) -> usize {
    steps
        .iter()
        .filter(|s| s.trigger.as_deref() == Some("MANUAL"))
        .filter(|s| matches!(s.status.to_uppercase().as_str(), "PENDING" | "READY"))
        .count()
}

/// The short (7-character) commit hash, when the pipeline has a commit target.
pub(super) fn get_commit_hash(pipeline: &Pipeline) -> Option<String> {
    pipeline
        .target
        .as_ref()
        .and_then(|t| t.commit.as_ref())
        .and_then(|c| c.hash.as_ref())
        .map(|h| h.chars().take(7).collect::<String>())
}

/// Whether `watch` and `status --wait` should stop polling.
///
/// Includes the states that wait on a person: a paused build does not finish
/// by itself, and polling it was an unbounded wait unless `--timeout` was set.
pub(super) fn is_terminal_state(status: &str) -> bool {
    is_awaiting_action(status)
        || matches!(
            status.to_uppercase().as_str(),
            "SUCCESSFUL" | "FAILED" | "STOPPED" | "ERROR" | "EXPIRED" | "COMPLETED"
        )
}

/// Map pipeline status to process exit code.
///
/// 0 = success, 1 = failed/stopped/error, 2 = in progress, pending or timed out,
/// 3 = paused, waiting on a manual step. Any other status is 1: an unrecognised
/// state is not evidence of success, and reporting it as 0 is how a broken
/// build passes a CI gate.
pub fn status_to_exit_code(status: &str) -> i32 {
    match status.to_uppercase().as_str() {
        "SUCCESSFUL" | "COMPLETED" => 0,
        "FAILED" | "ERROR" | "STOPPED" | "EXPIRED" => 1,
        "PENDING" | "IN_PROGRESS" | "RUNNING" | "TIMEOUT" => 2,
        "PAUSED" | "HALTED" => 3,
        _ => 1,
    }
}

pub(super) fn format_steps_summary(steps: &[StepInfo], use_colors: bool) -> String {
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

pub(super) fn format_elapsed(start: Instant) -> String {
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

pub(super) fn format_duration_secs(seconds: u64) -> String {
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
                trigger: None,
            },
            StepInfo {
                uuid: "{uuid2}".to_string(),
                name: "Build".to_string(),
                status: "IN_PROGRESS".to_string(),
                started: None,
                completed: None,
                duration: None,
                logs_url: None,
                trigger: None,
            },
            StepInfo {
                uuid: "{uuid3}".to_string(),
                name: "Deploy".to_string(),
                status: "PENDING".to_string(),
                started: None,
                completed: None,
                duration: None,
                logs_url: None,
                trigger: None,
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
    fn test_steps_empty_returns_empty_summary() {
        let steps: Vec<StepInfo> = vec![];
        let summary = format_steps_summary(&steps, false);
        assert!(summary.is_empty());
    }

    #[test]
    fn test_status_to_exit_code() {
        assert_eq!(status_to_exit_code("SUCCESSFUL"), 0);
        assert_eq!(status_to_exit_code("COMPLETED"), 0);
        assert_eq!(status_to_exit_code("FAILED"), 1);
        assert_eq!(status_to_exit_code("ERROR"), 1);
        assert_eq!(status_to_exit_code("STOPPED"), 1);
        assert_eq!(status_to_exit_code("EXPIRED"), 1);
        assert_eq!(status_to_exit_code("PENDING"), 2);
        assert_eq!(status_to_exit_code("IN_PROGRESS"), 2);
        // Case insensitive
        assert_eq!(status_to_exit_code("successful"), 0);
        assert_eq!(status_to_exit_code("failed"), 1);
        // Unknown fails closed
        assert_eq!(status_to_exit_code("UNKNOWN_STATUS"), 1);
        // Paused waits on a person
        assert_eq!(status_to_exit_code("PAUSED"), 3);
        assert_eq!(status_to_exit_code("halted"), 3);
        // Timeout maps to 2
        assert_eq!(status_to_exit_code("TIMEOUT"), 2);
    }

    fn pipeline(state: serde_json::Value) -> Pipeline {
        serde_json::from_value(serde_json::json!({"uuid": "{p}", "state": state})).unwrap()
    }

    /// The reported case: a build waiting on manual production deploys showed
    /// `IN_PROGRESS` for eleven days. The API says `PAUSED` under `stage`.
    #[test]
    fn a_paused_build_reports_paused_not_in_progress() {
        let paused = pipeline(serde_json::json!({
            "name": "IN_PROGRESS",
            "type": "pipeline_state_in_progress",
            "stage": {"name": "PAUSED", "type": "pipeline_state_in_progress_paused"}
        }));
        assert_eq!(get_pipeline_status(&paused), "PAUSED");
        assert!(
            is_terminal_state("PAUSED"),
            "watch must stop on a paused build"
        );
        assert_eq!(status_to_exit_code(&get_pipeline_status(&paused)), 3);
    }

    #[test]
    fn a_running_build_still_reports_in_progress() {
        let running = pipeline(serde_json::json!({
            "name": "IN_PROGRESS",
            "stage": {"name": "RUNNING"}
        }));
        assert_eq!(get_pipeline_status(&running), "IN_PROGRESS");
        let no_stage = pipeline(serde_json::json!({"name": "IN_PROGRESS"}));
        assert_eq!(get_pipeline_status(&no_stage), "IN_PROGRESS");
        assert!(!is_terminal_state("IN_PROGRESS"));
    }

    #[test]
    fn an_unexpected_stage_is_shown_rather_than_hidden() {
        let halted =
            pipeline(serde_json::json!({"name": "IN_PROGRESS", "stage": {"name": "HALTED"}}));
        assert_eq!(get_pipeline_status(&halted), "HALTED");
    }

    #[test]
    fn a_finished_build_reports_its_result() {
        let done = pipeline(serde_json::json!({
            "name": "COMPLETED",
            "result": {"name": "SUCCESSFUL"}
        }));
        assert_eq!(get_pipeline_status(&done), "SUCCESSFUL");
        let pending = pipeline(serde_json::json!({"name": "PENDING"}));
        assert_eq!(get_pipeline_status(&pending), "PENDING");
        let no_state: Pipeline =
            serde_json::from_value(serde_json::json!({"uuid": "{p}"})).unwrap();
        assert_eq!(get_pipeline_status(&no_state), "UNKNOWN");
    }

    /// A finished step's outcome lives in `result`; `state.name` is always
    /// `COMPLETED`, which is what `logs --failed-only` used to test.
    #[test]
    fn a_step_outcome_comes_from_its_result() {
        let failed: PipelineStep = serde_json::from_value(serde_json::json!({
            "uuid": "{s}",
            "state": {"name": "COMPLETED", "result": {"name": "FAILED"}}
        }))
        .unwrap();
        assert_eq!(get_step_status(&failed), "FAILED");
        let waiting: PipelineStep = serde_json::from_value(serde_json::json!({
            "uuid": "{s}",
            "state": {"name": "PENDING"}
        }))
        .unwrap();
        assert_eq!(get_step_status(&waiting), "PENDING");
    }

    #[test]
    fn trigger_types_lose_their_prefix() {
        assert_eq!(normalize_trigger("pipeline_step_trigger_manual"), "MANUAL");
        assert_eq!(
            normalize_trigger("pipeline_step_trigger_automatic"),
            "AUTOMATIC"
        );
        assert_eq!(normalize_trigger("MANUAL"), "MANUAL");
        assert_eq!(normalize_trigger("manual"), "MANUAL");
        assert_eq!(normalize_trigger("something_new"), "SOMETHING_NEW");
    }

    fn step(name: &str, status: &str, trigger: Option<&str>) -> StepInfo {
        StepInfo {
            uuid: format!("{{{name}}}"),
            name: name.to_string(),
            status: status.to_string(),
            started: None,
            completed: None,
            duration: None,
            logs_url: None,
            trigger: trigger.map(str::to_string),
        }
    }

    #[test]
    fn only_unstarted_manual_steps_count_as_pending() {
        let steps = vec![
            step("build", "SUCCESSFUL", Some("AUTOMATIC")),
            step("deploy-staging", "SUCCESSFUL", Some("MANUAL")),
            step("deploy-prod-a", "PENDING", Some("MANUAL")),
            step("deploy-prod-b", "READY", Some("MANUAL")),
            step("smoke", "PENDING", Some("AUTOMATIC")),
            step("legacy", "PENDING", None),
        ];
        assert_eq!(count_pending_manual_steps(&steps), 2);
        assert_eq!(count_pending_manual_steps(&[]), 0);
    }
}
