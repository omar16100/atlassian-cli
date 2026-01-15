#![allow(dead_code)]

use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use serde::Deserialize;

/// Context for OpsGenie operations.
pub struct OpsgenieContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}

/// OpsGenie API base URLs by region.
pub const OPSGENIE_US_API_URL: &str = "https://api.opsgenie.com/v2/";
pub const OPSGENIE_EU_API_URL: &str = "https://api.eu.opsgenie.com/v2/";

/// Alert priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    P1,
    P2,
    P3,
    P4,
    P5,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::P1 => write!(f, "P1"),
            Priority::P2 => write!(f, "P2"),
            Priority::P3 => write!(f, "P3"),
            Priority::P4 => write!(f, "P4"),
            Priority::P5 => write!(f, "P5"),
        }
    }
}

impl std::str::FromStr for Priority {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "P1" => Ok(Priority::P1),
            "P2" => Ok(Priority::P2),
            "P3" => Ok(Priority::P3),
            "P4" => Ok(Priority::P4),
            "P5" => Ok(Priority::P5),
            _ => Err(format!("Invalid priority: {}", s)),
        }
    }
}

/// Alert from OpsGenie API.
#[derive(Debug, Clone, Deserialize)]
pub struct Alert {
    pub id: String,
    #[serde(rename = "tinyId")]
    pub tiny_id: Option<String>,
    pub alias: Option<String>,
    pub message: String,
    pub status: Option<String>,
    pub acknowledged: Option<bool>,
    #[serde(rename = "isSeen")]
    pub is_seen: Option<bool>,
    pub snoozed: Option<bool>,
    pub priority: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
    pub source: Option<String>,
    pub owner: Option<String>,
    pub tags: Option<Vec<String>>,
    pub teams: Option<Vec<TeamMeta>>,
}

/// Team metadata in alert.
#[derive(Debug, Clone, Deserialize)]
pub struct TeamMeta {
    pub id: Option<String>,
    pub name: Option<String>,
}

/// Incident from OpsGenie API.
#[derive(Debug, Clone, Deserialize)]
pub struct Incident {
    pub id: String,
    #[serde(rename = "tinyId")]
    pub tiny_id: Option<String>,
    pub message: String,
    pub status: Option<String>,
    pub priority: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
    #[serde(rename = "impactedServices")]
    pub impacted_services: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

/// Schedule from OpsGenie API.
#[derive(Debug, Clone, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
    #[serde(rename = "ownerTeam")]
    pub owner_team: Option<TeamMeta>,
}

/// On-call participant.
#[derive(Debug, Clone, Deserialize)]
pub struct OnCallParticipant {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub participant_type: Option<String>,
}

/// Team from OpsGenie API.
#[derive(Debug, Clone, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// Team member.
#[derive(Debug, Clone, Deserialize)]
pub struct TeamMember {
    pub user: TeamUser,
    pub role: Option<String>,
}

/// Team user.
#[derive(Debug, Clone, Deserialize)]
pub struct TeamUser {
    pub id: Option<String>,
    pub username: Option<String>,
}

/// Escalation policy.
#[derive(Debug, Clone, Deserialize)]
pub struct Escalation {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "ownerTeam")]
    pub owner_team: Option<TeamMeta>,
    pub rules: Option<Vec<EscalationRule>>,
}

/// Escalation rule.
#[derive(Debug, Clone, Deserialize)]
pub struct EscalationRule {
    pub condition: Option<String>,
    #[serde(rename = "notifyType")]
    pub notify_type: Option<String>,
    pub delay: Option<EscalationDelay>,
    pub recipient: Option<Recipient>,
}

/// Escalation delay.
#[derive(Debug, Clone, Deserialize)]
pub struct EscalationDelay {
    #[serde(rename = "timeAmount")]
    pub time_amount: Option<i32>,
    #[serde(rename = "timeUnit")]
    pub time_unit: Option<String>,
}

/// Recipient for notifications.
#[derive(Debug, Clone, Deserialize)]
pub struct Recipient {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub recipient_type: Option<String>,
}

/// Service definition.
#[derive(Debug, Clone, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "teamId")]
    pub team_id: Option<String>,
}

/// Heartbeat definition.
#[derive(Debug, Clone, Deserialize)]
pub struct Heartbeat {
    pub name: String,
    pub description: Option<String>,
    pub interval: Option<i32>,
    #[serde(rename = "intervalUnit")]
    pub interval_unit: Option<String>,
    pub enabled: Option<bool>,
    pub expired: Option<bool>,
    #[serde(rename = "ownerTeam")]
    pub owner_team: Option<TeamMeta>,
}

/// Wrapper for OpsGenie API responses.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    pub took: Option<f64>,
}

/// Paged API response.
#[derive(Debug, Clone, Deserialize)]
pub struct PagedResponse<T> {
    pub data: Vec<T>,
    pub paging: Option<Paging>,
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    pub took: Option<f64>,
}

/// Paging info.
#[derive(Debug, Clone, Deserialize)]
pub struct Paging {
    pub next: Option<String>,
    pub first: Option<String>,
    pub last: Option<String>,
}
