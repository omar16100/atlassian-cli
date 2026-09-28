//! Bitbucket Pipelines API response types and the rows rendered from them.

use serde::{Deserialize, Serialize};

// ============================================================================
// API Response Structs
// ============================================================================

#[derive(Deserialize)]
pub(super) struct PipelineList {
    pub(super) values: Vec<Pipeline>,
    pub(super) next: Option<String>,
    #[allow(dead_code)]
    pub(super) page: Option<u32>,
    #[allow(dead_code)]
    pub(super) pagelen: Option<u32>,
    #[allow(dead_code)]
    pub(super) size: Option<u32>,
}

#[derive(Deserialize, Clone)]
pub(super) struct Pipeline {
    pub(super) uuid: String,
    #[serde(default)]
    pub(super) build_number: Option<i64>,
    #[serde(default)]
    pub(super) state: Option<PipelineState>,
    #[serde(default)]
    pub(super) created_on: Option<String>,
    #[serde(default)]
    pub(super) completed_on: Option<String>,
    #[serde(default)]
    pub(super) target: Option<Target>,
}

#[derive(Deserialize, Clone)]
pub(super) struct PipelineState {
    pub(super) name: String,
    #[serde(default)]
    pub(super) result: Option<StateResult>,
}

#[derive(Deserialize, Clone)]
pub(super) struct StateResult {
    pub(super) name: String,
}

#[derive(Deserialize, Clone)]
pub(super) struct Target {
    #[serde(default)]
    pub(super) ref_name: Option<String>,
    #[serde(rename = "type", default)]
    pub(super) target_type: Option<String>,
    #[serde(default)]
    pub(super) commit: Option<CommitInfo>,
}

#[derive(Deserialize, Clone)]
pub(super) struct CommitInfo {
    #[serde(default)]
    pub(super) hash: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(super) struct PipelineStep {
    pub(super) uuid: String,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) state: Option<StepState>,
    #[serde(default)]
    pub(super) started_on: Option<String>,
    #[serde(default)]
    pub(super) completed_on: Option<String>,
    #[serde(default)]
    pub(super) duration_in_seconds: Option<u64>,
    #[serde(default)]
    pub(super) trigger: Option<StepTrigger>,
}

#[derive(Deserialize, Clone)]
pub(super) struct StepTrigger {
    #[serde(rename = "type")]
    pub(super) trigger_type: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(super) struct StepState {
    pub(super) name: String,
    #[serde(default)]
    pub(super) result: Option<StepResult>,
}

#[derive(Deserialize, Clone)]
pub(super) struct StepResult {
    pub(super) name: String,
}

// ============================================================================
// Output Structs
// ============================================================================

#[derive(Serialize)]
pub(super) struct PipelineRow {
    pub(super) build_number: String,
    pub(super) state: String,
    pub(super) ref_name: String,
    pub(super) commit: String,
    pub(super) target_type: String,
    pub(super) created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) steps_summary: Option<String>,
}

#[derive(Serialize)]
pub(super) struct PipelineView {
    pub(super) uuid: String,
    pub(super) build_number: String,
    pub(super) state: String,
    pub(super) ref_name: String,
    pub(super) commit: String,
    pub(super) created: String,
    pub(super) completed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) steps: Option<Vec<StepInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) steps_summary: Option<String>,
}

#[derive(Serialize, Clone)]
pub(super) struct StepInfo {
    pub(super) uuid: String,
    pub(super) name: String,
    pub(super) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) started: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) completed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) logs_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) trigger: Option<String>,
}

#[derive(Serialize)]
pub(super) struct PipelineStatusOutput {
    pub(super) build_number: i64,
    pub(super) state: String,
    pub(super) ref_name: String,
    pub(super) commit: String,
    pub(super) created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) steps: Option<Vec<StepInfo>>,
}

#[derive(Serialize, Clone, Debug)]
pub(super) struct PipelineVariable {
    pub(super) key: String,
    pub(super) value: String,
    pub(super) secured: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dropping it from the table must not drop it from the data. A script
    /// reading `-f json` still needs the link.
    #[test]
    fn a_step_still_serialises_its_logs_url() {
        let step = StepInfo {
            uuid: "{s-1}".to_string(),
            name: "build".to_string(),
            status: "SUCCESSFUL".to_string(),
            started: None,
            completed: None,
            duration: None,
            logs_url: Some("https://bitbucket.org/w/r/pipelines/results/1/steps/2".to_string()),
            trigger: None,
        };
        let value = serde_json::to_value(&step).unwrap();
        assert!(
            value.get("logs_url").is_some(),
            "the structured formats must keep it: {value}"
        );
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

    #[test]
    fn test_step_trigger_deserialization() {
        let json = r#"{
            "uuid": "{step-uuid}",
            "name": "Deploy",
            "trigger": {"type": "manual"}
        }"#;
        let step: PipelineStep = serde_json::from_str(json).unwrap();
        assert_eq!(step.trigger.unwrap().trigger_type.unwrap(), "manual");
    }

    #[test]
    fn test_step_without_trigger_deserialization() {
        let json = r#"{
            "uuid": "{step-uuid}",
            "name": "Build"
        }"#;
        let step: PipelineStep = serde_json::from_str(json).unwrap();
        assert!(step.trigger.is_none());
    }
}
