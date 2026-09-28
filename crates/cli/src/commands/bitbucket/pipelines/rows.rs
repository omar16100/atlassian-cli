//! Pipelines as rendered: decorated for people, plain for scripts.
//!
//! One row type serves every format, so the decoration has to be decided when
//! the row is built. It used to be applied unconditionally, which is how
//! `-f json` came to carry `"state": "IN_PROGRESS 🔄"` and `"completed": ""`.
//! Here the human formats get icons (and colour in a table), and the machine
//! formats get the raw status, numbers as numbers and `null` for anything the
//! API did not report.

use atlassian_cli_output::OutputFormat;

use super::model::{Pipeline, PipelineRow, PipelineView, StepInfo};
use super::state::{
    count_pending_manual_steps, format_status_for_display, format_steps_summary, get_commit_hash,
    get_pipeline_status, is_awaiting_action,
};

/// The state cell: coloured with an icon in a table, an icon in markdown, the
/// bare status everywhere else.
///
/// Markdown gets no colour: ANSI escapes in a document are noise.
pub(super) fn display_state(status: &str, format: OutputFormat) -> String {
    match format {
        OutputFormat::Table => format_status_for_display(status, true),
        OutputFormat::Markdown => format_status_for_display(status, false),
        _ => status.to_string(),
    }
}

/// `name STATUS` per step, joined by ` | `. With icons (and colour in a table)
/// for the human formats; `None` when there are no steps to summarise.
pub(super) fn steps_summary(steps: &[StepInfo], format: OutputFormat) -> Option<String> {
    if steps.is_empty() {
        return None;
    }
    Some(match format {
        OutputFormat::Table => format_steps_summary(steps, true),
        OutputFormat::Markdown => format_steps_summary(steps, false),
        _ => steps
            .iter()
            .map(|s| format!("{} {}", s.name, s.status))
            .collect::<Vec<_>>()
            .join(" | "),
    })
}

/// How many manual steps a paused build is waiting on. `None` for any other
/// build, and when its steps were not fetched.
pub(super) fn pending_manual_steps(status: &str, steps: Option<&[StepInfo]>) -> Option<usize> {
    if is_awaiting_action(status) {
        steps.map(count_pending_manual_steps)
    } else {
        None
    }
}

fn ref_name(pipeline: &Pipeline) -> Option<String> {
    pipeline.target.as_ref().and_then(|t| t.ref_name.clone())
}

/// One row of `pipeline list`. `steps` is only present with `--steps`.
pub(super) fn build_pipeline_row(
    pipeline: &Pipeline,
    steps: Option<&[StepInfo]>,
    format: OutputFormat,
) -> PipelineRow {
    let status = get_pipeline_status(pipeline);
    PipelineRow {
        build_number: pipeline.build_number,
        state: display_state(&status, format),
        ref_name: ref_name(pipeline),
        commit: get_commit_hash(pipeline),
        target_type: pipeline.target.as_ref().and_then(|t| t.target_type.clone()),
        created: pipeline.created_on.clone(),
        steps_summary: steps.and_then(|s| steps_summary(s, format)),
        pending_manual_steps: pending_manual_steps(&status, steps),
    }
}

/// The single-pipeline view shared by `pipeline get` and `watch`.
///
/// `steps` may have been fetched only to count what a paused build waits on;
/// `show_steps` decides whether they are also listed.
pub(super) fn build_pipeline_view(
    pipeline: &Pipeline,
    steps: Option<Vec<StepInfo>>,
    show_steps: bool,
    format: OutputFormat,
) -> PipelineView {
    let status = get_pipeline_status(pipeline);
    let pending = pending_manual_steps(&status, steps.as_deref());
    let steps = if show_steps { steps } else { None };
    PipelineView {
        uuid: pipeline.uuid.clone(),
        build_number: pipeline.build_number,
        state: display_state(&status, format),
        ref_name: ref_name(pipeline),
        commit: get_commit_hash(pipeline),
        created: pipeline.created_on.clone(),
        completed: pipeline.completed_on.clone(),
        steps_summary: steps.as_deref().and_then(|s| steps_summary(s, format)),
        steps,
        pending_manual_steps: pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paused_pipeline() -> Pipeline {
        serde_json::from_value(serde_json::json!({
            "uuid": "{p-592}",
            "build_number": 592,
            "state": {"name": "IN_PROGRESS", "stage": {"name": "PAUSED"}},
            "created_on": "2026-09-17T02:00:00Z",
            "completed_on": null,
            "target": {"ref_name": "main", "type": "pipeline_ref_target", "commit": {"hash": "0123456789abcdef"}}
        }))
        .unwrap()
    }

    fn step(name: &str, status: &str, trigger: &str) -> StepInfo {
        StepInfo {
            uuid: format!("{{{name}}}"),
            name: name.to_string(),
            status: status.to_string(),
            started: None,
            completed: None,
            duration: None,
            logs_url: None,
            trigger: Some(trigger.to_string()),
        }
    }

    fn steps() -> Vec<StepInfo> {
        vec![
            step("build", "SUCCESSFUL", "AUTOMATIC"),
            step("deploy-prod", "PENDING", "MANUAL"),
        ]
    }

    /// The reported JSON: `"state": "IN_PROGRESS 🔄"`, `"completed": ""`.
    #[test]
    fn json_view_carries_plain_values() {
        let view = build_pipeline_view(&paused_pipeline(), Some(steps()), true, OutputFormat::Json);
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["state"], "PAUSED");
        assert_eq!(json["build_number"], 592, "a number, not a string");
        assert!(
            json["completed"].is_null(),
            "null, not an empty string: {json}"
        );
        assert_eq!(json["commit"], "0123456");
        assert_eq!(json["pending_manual_steps"], 1);
        assert_eq!(
            json["steps_summary"],
            "build SUCCESSFUL | deploy-prod PENDING"
        );
        let text = json.to_string();
        for icon in ["🔄", "⏸", "✅", "⏳", "❓"] {
            assert!(!text.contains(icon), "no {icon} in machine output: {text}");
        }
    }

    #[test]
    fn yaml_and_csv_rows_are_plain_too() {
        for format in [OutputFormat::Yaml, OutputFormat::Csv, OutputFormat::Quiet] {
            let row = build_pipeline_row(&paused_pipeline(), None, format);
            assert_eq!(row.state, "PAUSED", "{format:?}");
        }
    }

    #[test]
    fn human_views_keep_their_icons() {
        let view = build_pipeline_view(&paused_pipeline(), None, false, OutputFormat::Markdown);
        assert_eq!(view.state, "PAUSED ⏸");
        let row = build_pipeline_row(&paused_pipeline(), Some(&steps()), OutputFormat::Markdown);
        assert!(row.steps_summary.unwrap().contains('✅'));
    }

    #[test]
    fn missing_values_are_null_not_empty() {
        let bare: Pipeline = serde_json::from_value(serde_json::json!({"uuid": "{p}"})).unwrap();
        let json =
            serde_json::to_value(build_pipeline_row(&bare, None, OutputFormat::Json)).unwrap();
        for key in [
            "build_number",
            "ref_name",
            "commit",
            "target_type",
            "created",
        ] {
            assert!(json[key].is_null(), "{key} should be null: {json}");
        }
        assert!(json.get("steps_summary").is_none());
        assert!(json.get("pending_manual_steps").is_none());
    }

    /// Steps fetched only to count them are not listed unless asked for.
    #[test]
    fn steps_fetched_for_the_count_are_not_listed_without_steps() {
        let view =
            build_pipeline_view(&paused_pipeline(), Some(steps()), false, OutputFormat::Json);
        assert_eq!(view.pending_manual_steps, Some(1));
        assert!(view.steps.is_none());
        assert!(view.steps_summary.is_none());
    }

    #[test]
    fn a_running_build_reports_no_pending_count() {
        let running: Pipeline = serde_json::from_value(serde_json::json!({
            "uuid": "{p}",
            "state": {"name": "IN_PROGRESS", "stage": {"name": "RUNNING"}}
        }))
        .unwrap();
        let view = build_pipeline_view(&running, Some(steps()), true, OutputFormat::Json);
        assert_eq!(view.state, "IN_PROGRESS");
        assert_eq!(view.pending_manual_steps, None);
    }
}
