use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand, ValueEnum};

mod alerts;
mod escalations;
mod heartbeats;
mod incidents;
mod schedules;
mod services;
mod teams;
pub mod utils;

pub use utils::OpsgenieContext;

/// OpsGenie region for API endpoints.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum Region {
    #[default]
    Us,
    Eu,
}

impl Region {
    pub fn base_url(&self) -> &'static str {
        match self {
            Region::Us => utils::OPSGENIE_US_API_URL,
            Region::Eu => utils::OPSGENIE_EU_API_URL,
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct OpsgenieArgs {
    #[command(subcommand)]
    command: OpsgenieCommands,
}

#[derive(Subcommand, Debug, Clone)]
enum OpsgenieCommands {
    /// Alert operations.
    #[command(subcommand)]
    Alert(AlertCommands),

    /// Incident operations.
    #[command(subcommand)]
    Incident(IncidentCommands),

    /// Schedule operations.
    #[command(subcommand)]
    Schedule(ScheduleCommands),

    /// Team operations.
    #[command(subcommand)]
    Team(TeamCommands),

    /// Escalation policy operations.
    #[command(subcommand)]
    Escalation(EscalationCommands),

    /// Service operations.
    #[command(subcommand)]
    Service(ServiceCommands),

    /// Heartbeat operations.
    #[command(subcommand)]
    Heartbeat(HeartbeatCommands),
}

#[derive(Subcommand, Debug, Clone)]
enum AlertCommands {
    /// List alerts.
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long)]
        status: Option<String>,
    },
    /// Get alert details.
    Get {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Create a new alert.
    Create {
        #[arg(long)]
        message: String,
        #[arg(long)]
        alias: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        tags: Vec<String>,
    },
    /// Close an alert.
    Close {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Acknowledge an alert.
    Ack {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Unacknowledge an alert.
    Unack {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Snooze an alert.
    Snooze {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        end_time: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Escalate an alert.
    Escalate {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        escalation_id: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Assign an alert to a user.
    Assign {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        owner_id: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Add a note to an alert.
    AddNote {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        note: String,
    },
    /// Delete an alert.
    Delete {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// List alert recipients.
    Recipients {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// List alert logs.
    Logs {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// List alert notes.
    Notes {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum IncidentCommands {
    /// List incidents.
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get incident details.
    Get {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Create a new incident.
    Create {
        #[arg(long)]
        message: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        tags: Vec<String>,
        #[arg(long)]
        service_ids: Vec<String>,
    },
    /// Close an incident.
    Close {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Resolve an incident.
    Resolve {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Reopen an incident.
    Reopen {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Add responder to incident.
    AddResponder {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        responder_id: String,
        #[arg(long, default_value = "user")]
        responder_type: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Add note to incident.
    AddNote {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        note: String,
    },
    /// Delete an incident.
    Delete {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// List incident timeline.
    Timeline {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum ScheduleCommands {
    /// List schedules.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get schedule details.
    Get {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Create a new schedule.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        timezone: Option<String>,
        #[arg(long)]
        team_id: Option<String>,
    },
    /// Delete a schedule.
    Delete {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Enable a schedule.
    Enable {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Disable a schedule.
    Disable {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Get who is on-call.
    OnCall {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        date: Option<String>,
    },
    /// Get schedule timeline.
    Timeline {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        interval: Option<i32>,
        #[arg(long)]
        interval_unit: Option<String>,
        #[arg(long)]
        date: Option<String>,
    },
    /// Export schedule to iCal.
    Export {
        #[arg(value_name = "ID")]
        identifier: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum TeamCommands {
    /// List teams.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get team details.
    Get {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Create a new team.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete a team.
    Delete {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// List team members.
    Members {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Add member to team.
    AddMember {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        user_id: String,
        #[arg(long)]
        role: Option<String>,
    },
    /// Remove member from team.
    RemoveMember {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        user_id: String,
    },
    /// Get team's on-call participants.
    OnCall {
        #[arg(value_name = "ID")]
        identifier: String,
        #[arg(long)]
        date: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum EscalationCommands {
    /// List escalation policies.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get escalation policy details.
    Get {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Create a new escalation policy.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        team_id: Option<String>,
    },
    /// Delete an escalation policy.
    Delete {
        #[arg(value_name = "ID")]
        identifier: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum ServiceCommands {
    /// List services.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get service details.
    Get {
        #[arg(value_name = "ID")]
        identifier: String,
    },
    /// Create a new service.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        team_id: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete a service.
    Delete {
        #[arg(value_name = "ID")]
        identifier: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum HeartbeatCommands {
    /// List heartbeats.
    List,
    /// Get heartbeat details.
    Get {
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Create a new heartbeat.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, default_value_t = 10)]
        interval: i32,
        #[arg(long, default_value = "minutes")]
        interval_unit: String,
        #[arg(long)]
        team_id: Option<String>,
    },
    /// Delete a heartbeat.
    Delete {
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Enable a heartbeat.
    Enable {
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Disable a heartbeat.
    Disable {
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Send a ping to a heartbeat.
    Ping {
        #[arg(value_name = "NAME")]
        name: String,
    },
}

/// Execute OpsGenie command.
pub async fn execute(
    args: OpsgenieArgs,
    client: ApiClient,
    renderer: &OutputRenderer,
) -> Result<()> {
    let ctx = OpsgenieContext { client, renderer };

    match args.command {
        OpsgenieCommands::Alert(cmd) => match cmd {
            AlertCommands::List {
                query,
                limit,
                status,
            } => alerts::list_alerts(&ctx, query.as_deref(), limit, status.as_deref()).await,
            AlertCommands::Get { identifier } => alerts::get_alert(&ctx, &identifier).await,
            AlertCommands::Create {
                message,
                alias,
                description,
                priority,
                source,
                tags,
            } => {
                alerts::create_alert(&ctx, message, alias, description, priority, source, tags)
                    .await
            }
            AlertCommands::Close { identifier, note } => {
                alerts::close_alert(&ctx, &identifier, note).await
            }
            AlertCommands::Ack { identifier, note } => {
                alerts::acknowledge_alert(&ctx, &identifier, note).await
            }
            AlertCommands::Unack { identifier, note } => {
                alerts::unacknowledge_alert(&ctx, &identifier, note).await
            }
            AlertCommands::Snooze {
                identifier,
                end_time,
                note,
            } => alerts::snooze_alert(&ctx, &identifier, &end_time, note).await,
            AlertCommands::Escalate {
                identifier,
                escalation_id,
                note,
            } => alerts::escalate_alert(&ctx, &identifier, &escalation_id, note).await,
            AlertCommands::Assign {
                identifier,
                owner_id,
                note,
            } => alerts::assign_alert(&ctx, &identifier, &owner_id, note).await,
            AlertCommands::AddNote { identifier, note } => {
                alerts::add_note(&ctx, &identifier, &note).await
            }
            AlertCommands::Delete { identifier } => alerts::delete_alert(&ctx, &identifier).await,
            AlertCommands::Recipients { identifier } => {
                alerts::list_recipients(&ctx, &identifier).await
            }
            AlertCommands::Logs { identifier, limit } => {
                alerts::list_logs(&ctx, &identifier, limit).await
            }
            AlertCommands::Notes { identifier, limit } => {
                alerts::list_notes(&ctx, &identifier, limit).await
            }
        },
        OpsgenieCommands::Incident(cmd) => match cmd {
            IncidentCommands::List { query, limit } => {
                incidents::list_incidents(&ctx, query.as_deref(), limit).await
            }
            IncidentCommands::Get { identifier } => {
                incidents::get_incident(&ctx, &identifier).await
            }
            IncidentCommands::Create {
                message,
                description,
                priority,
                tags,
                service_ids,
            } => {
                incidents::create_incident(&ctx, message, description, priority, tags, service_ids)
                    .await
            }
            IncidentCommands::Close { identifier, note } => {
                incidents::close_incident(&ctx, &identifier, note).await
            }
            IncidentCommands::Resolve { identifier, note } => {
                incidents::resolve_incident(&ctx, &identifier, note).await
            }
            IncidentCommands::Reopen { identifier, note } => {
                incidents::reopen_incident(&ctx, &identifier, note).await
            }
            IncidentCommands::AddResponder {
                identifier,
                responder_id,
                responder_type,
                note,
            } => {
                incidents::add_responder(&ctx, &identifier, &responder_id, &responder_type, note)
                    .await
            }
            IncidentCommands::AddNote { identifier, note } => {
                incidents::add_note(&ctx, &identifier, &note).await
            }
            IncidentCommands::Delete { identifier } => {
                incidents::delete_incident(&ctx, &identifier).await
            }
            IncidentCommands::Timeline { identifier, limit } => {
                incidents::list_timeline(&ctx, &identifier, limit).await
            }
        },
        OpsgenieCommands::Schedule(cmd) => match cmd {
            ScheduleCommands::List { limit } => schedules::list_schedules(&ctx, limit).await,
            ScheduleCommands::Get { identifier } => {
                schedules::get_schedule(&ctx, &identifier).await
            }
            ScheduleCommands::Create {
                name,
                description,
                timezone,
                team_id,
            } => schedules::create_schedule(&ctx, name, description, timezone, team_id).await,
            ScheduleCommands::Delete { identifier } => {
                schedules::delete_schedule(&ctx, &identifier).await
            }
            ScheduleCommands::Enable { identifier } => {
                schedules::enable_schedule(&ctx, &identifier).await
            }
            ScheduleCommands::Disable { identifier } => {
                schedules::disable_schedule(&ctx, &identifier).await
            }
            ScheduleCommands::OnCall { identifier, date } => {
                schedules::get_on_call(&ctx, &identifier, date.as_deref()).await
            }
            ScheduleCommands::Timeline {
                identifier,
                interval,
                interval_unit,
                date,
            } => {
                schedules::get_timeline(
                    &ctx,
                    &identifier,
                    interval,
                    interval_unit.as_deref(),
                    date.as_deref(),
                )
                .await
            }
            ScheduleCommands::Export { identifier } => {
                schedules::export_ical(&ctx, &identifier).await
            }
        },
        OpsgenieCommands::Team(cmd) => match cmd {
            TeamCommands::List { limit } => teams::list_teams(&ctx, limit).await,
            TeamCommands::Get { identifier } => teams::get_team(&ctx, &identifier).await,
            TeamCommands::Create { name, description } => {
                teams::create_team(&ctx, name, description).await
            }
            TeamCommands::Delete { identifier } => teams::delete_team(&ctx, &identifier).await,
            TeamCommands::Members { identifier } => teams::list_members(&ctx, &identifier).await,
            TeamCommands::AddMember {
                identifier,
                user_id,
                role,
            } => teams::add_member(&ctx, &identifier, &user_id, role.as_deref()).await,
            TeamCommands::RemoveMember {
                identifier,
                user_id,
            } => teams::remove_member(&ctx, &identifier, &user_id).await,
            TeamCommands::OnCall { identifier, date } => {
                teams::get_on_call(&ctx, &identifier, date.as_deref()).await
            }
        },
        OpsgenieCommands::Escalation(cmd) => match cmd {
            EscalationCommands::List { limit } => escalations::list_escalations(&ctx, limit).await,
            EscalationCommands::Get { identifier } => {
                escalations::get_escalation(&ctx, &identifier).await
            }
            EscalationCommands::Create {
                name,
                description,
                team_id,
            } => escalations::create_escalation(&ctx, name, description, team_id).await,
            EscalationCommands::Delete { identifier } => {
                escalations::delete_escalation(&ctx, &identifier).await
            }
        },
        OpsgenieCommands::Service(cmd) => match cmd {
            ServiceCommands::List { limit } => services::list_services(&ctx, limit).await,
            ServiceCommands::Get { identifier } => services::get_service(&ctx, &identifier).await,
            ServiceCommands::Create {
                name,
                team_id,
                description,
            } => services::create_service(&ctx, name, team_id, description).await,
            ServiceCommands::Delete { identifier } => {
                services::delete_service(&ctx, &identifier).await
            }
        },
        OpsgenieCommands::Heartbeat(cmd) => match cmd {
            HeartbeatCommands::List => heartbeats::list_heartbeats(&ctx).await,
            HeartbeatCommands::Get { name } => heartbeats::get_heartbeat(&ctx, &name).await,
            HeartbeatCommands::Create {
                name,
                description,
                interval,
                interval_unit,
                team_id,
            } => {
                heartbeats::create_heartbeat(
                    &ctx,
                    name,
                    description,
                    interval,
                    interval_unit,
                    team_id,
                )
                .await
            }
            HeartbeatCommands::Delete { name } => heartbeats::delete_heartbeat(&ctx, &name).await,
            HeartbeatCommands::Enable { name } => heartbeats::enable_heartbeat(&ctx, &name).await,
            HeartbeatCommands::Disable { name } => heartbeats::disable_heartbeat(&ctx, &name).await,
            HeartbeatCommands::Ping { name } => heartbeats::ping_heartbeat(&ctx, &name).await,
        },
    }
}
