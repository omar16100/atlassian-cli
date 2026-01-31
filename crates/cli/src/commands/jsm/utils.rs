#![allow(dead_code)]

use anyhow::Context;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use serde::Deserialize;

/// Context for JSM command execution.
pub struct JsmContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}

impl JsmContext<'_> {
    pub async fn verify_auth(&self) -> anyhow::Result<()> {
        let _: serde_json::Value =
            self.client.get("/rest/api/3/myself").await.context(
                "Authentication may be expired or invalid. Run: atlassian-cli auth test",
            )?;
        Ok(())
    }
}

/// Request field from JSM API response.
#[derive(Deserialize, Debug, Clone)]
pub struct RequestField {
    #[serde(rename = "fieldId")]
    pub field_id: String,
    #[serde(rename = "label")]
    pub label: String,
    #[serde(rename = "value", default)]
    pub value: Option<String>,
}

/// Reporter information.
#[derive(Deserialize, Debug, Clone)]
pub struct RequestReporter {
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "accountId", default)]
    pub account_id: Option<String>,
    #[serde(rename = "emailAddress", default)]
    pub email_address: Option<String>,
}

/// Request status information.
#[derive(Deserialize, Debug, Clone)]
pub struct RequestStatus {
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "statusCategory", default)]
    pub status_category: Option<StatusCategory>,
}

/// Status category.
#[derive(Deserialize, Debug, Clone)]
pub struct StatusCategory {
    #[serde(rename = "key")]
    pub key: String,
    #[serde(rename = "colorName", default)]
    pub color_name: Option<String>,
}

/// User information (for customers, participants).
#[derive(Deserialize, Debug, Clone)]
pub struct User {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "emailAddress", default)]
    pub email_address: Option<String>,
    #[serde(default)]
    pub active: bool,
}

/// Organization information.
#[derive(Deserialize, Debug, Clone)]
pub struct Organization {
    pub id: String,
    pub name: String,
}

/// SLA information.
#[derive(Deserialize, Debug, Clone)]
pub struct SlaInformation {
    pub id: String,
    pub name: String,
    #[serde(rename = "completedCycles", default)]
    pub completed_cycles: Vec<SlaCycle>,
    #[serde(rename = "ongoingCycle", default)]
    pub ongoing_cycle: Option<SlaCycle>,
}

/// SLA cycle information.
#[derive(Deserialize, Debug, Clone)]
pub struct SlaCycle {
    #[serde(rename = "startTime", default)]
    pub start_time: Option<DateDto>,
    #[serde(rename = "stopTime", default)]
    pub stop_time: Option<DateDto>,
    #[serde(rename = "breached", default)]
    pub breached: bool,
    #[serde(rename = "goalDuration", default)]
    pub goal_duration: Option<Duration>,
    #[serde(rename = "elapsedTime", default)]
    pub elapsed_time: Option<Duration>,
    #[serde(rename = "remainingTime", default)]
    pub remaining_time: Option<Duration>,
}

/// Date DTO from JSM API.
#[derive(Deserialize, Debug, Clone)]
pub struct DateDto {
    pub iso8601: String,
    #[serde(rename = "epochMillis")]
    pub epoch_millis: i64,
}

/// Duration from JSM API.
#[derive(Deserialize, Debug, Clone)]
pub struct Duration {
    pub millis: i64,
    pub friendly: String,
}

/// Comment on a request.
#[derive(Deserialize, Debug, Clone)]
pub struct Comment {
    pub id: String,
    pub body: String,
    #[serde(default)]
    pub public: bool,
    #[serde(rename = "created", default)]
    pub created: Option<DateDto>,
    #[serde(default)]
    pub author: Option<User>,
}

/// Approval information.
#[derive(Deserialize, Debug, Clone)]
pub struct Approval {
    pub id: String,
    pub name: String,
    #[serde(rename = "finalDecision", default)]
    pub final_decision: Option<String>,
    #[serde(rename = "canAnswerApproval", default)]
    pub can_answer_approval: bool,
    #[serde(default)]
    pub approvers: Vec<Approver>,
}

/// Approver information.
#[derive(Deserialize, Debug, Clone)]
pub struct Approver {
    #[serde(default)]
    pub approver: Option<User>,
    #[serde(rename = "approverDecision", default)]
    pub approver_decision: Option<String>,
}

/// Queue information.
#[derive(Deserialize, Debug, Clone)]
pub struct Queue {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub jql: Option<String>,
    #[serde(rename = "issueCount", default)]
    pub issue_count: i64,
}

/// Request type information.
#[derive(Deserialize, Debug, Clone)]
pub struct RequestType {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "serviceDeskId")]
    pub service_desk_id: String,
    #[serde(rename = "groupIds", default)]
    pub group_ids: Vec<String>,
}

/// Request type field.
#[derive(Deserialize, Debug, Clone)]
pub struct RequestTypeField {
    #[serde(rename = "fieldId")]
    pub field_id: String,
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(rename = "validValues", default)]
    pub valid_values: Vec<FieldValue>,
}

/// Field value for request type fields.
#[derive(Deserialize, Debug, Clone)]
pub struct FieldValue {
    pub value: String,
    #[serde(default)]
    pub label: Option<String>,
}

/// Knowledge base article.
#[derive(Deserialize, Debug, Clone)]
pub struct KbArticle {
    pub title: String,
    pub excerpt: String,
    #[serde(default)]
    pub source: Option<KbArticleSource>,
}

/// KB article source.
#[derive(Deserialize, Debug, Clone)]
pub struct KbArticleSource {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(rename = "pageId", default)]
    pub page_id: Option<String>,
}

/// Feedback information.
#[derive(Deserialize, Debug, Clone)]
pub struct Feedback {
    #[serde(default)]
    pub rating: Option<i32>,
    #[serde(default)]
    pub comment: Option<Comment>,
}

/// Transition information.
#[derive(Deserialize, Debug, Clone)]
pub struct Transition {
    pub id: String,
    pub name: String,
}

/// Helper function to extract field value from request fields.
pub fn field_value<'a>(fields: &'a [RequestField], id_or_label: &str) -> &'a str {
    fields
        .iter()
        .find_map(|field| {
            if field.field_id.eq_ignore_ascii_case(id_or_label)
                || field.label.eq_ignore_ascii_case(id_or_label)
            {
                field.value.as_deref()
            } else {
                None
            }
        })
        .unwrap_or("")
}
