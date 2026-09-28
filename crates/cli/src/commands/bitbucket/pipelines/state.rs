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
        "PENDING" | "NOT_RUN" => "⏳",
        "PAUSED" => "⏸",
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

pub(super) fn get_step_status(step: &PipelineStep) -> String {
    step.state
        .as_ref()
        .and_then(|s| s.result.as_ref().map(|r| r.name.clone()))
        .or_else(|| step.state.as_ref().map(|s| s.name.clone()))
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

pub(super) fn get_pipeline_status(pipeline: &Pipeline) -> String {
    pipeline
        .state
        .as_ref()
        .and_then(|s| s.result.as_ref().map(|r| r.name.clone()))
        .or_else(|| pipeline.state.as_ref().map(|s| s.name.clone()))
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

pub(super) fn get_commit_hash(pipeline: &Pipeline) -> String {
    pipeline
        .target
        .as_ref()
        .and_then(|t| t.commit.as_ref())
        .and_then(|c| c.hash.as_ref())
        .map(|h| h.chars().take(7).collect::<String>())
        .unwrap_or_default()
}

pub(super) fn is_terminal_state(status: &str) -> bool {
    matches!(
        status.to_uppercase().as_str(),
        "SUCCESSFUL" | "FAILED" | "STOPPED" | "ERROR" | "EXPIRED" | "COMPLETED"
    )
}

/// Map pipeline status to process exit code.
/// 0 = success, 1 = failed/stopped/error, 2 = in-progress/pending.
pub fn status_to_exit_code(status: &str) -> i32 {
    match status.to_uppercase().as_str() {
        "SUCCESSFUL" | "COMPLETED" => 0,
        "FAILED" | "ERROR" | "STOPPED" | "EXPIRED" => 1,
        "PENDING" | "IN_PROGRESS" | "TIMEOUT" => 2,
        _ => 0,
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
        // Unknown defaults to 0
        assert_eq!(status_to_exit_code("UNKNOWN_STATUS"), 0);
        // Timeout maps to 2
        assert_eq!(status_to_exit_code("TIMEOUT"), 2);
    }
}
