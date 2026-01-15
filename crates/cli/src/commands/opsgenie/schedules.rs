use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{ApiResponse, OnCallParticipant, OpsgenieContext, PagedResponse, Schedule};

/// List all schedules.
pub async fn list_schedules(ctx: &OpsgenieContext<'_>, limit: usize) -> Result<()> {
    let path = format!("schedules?limit={}", limit.min(100));
    tracing::debug!("Listing schedules with limit {}", limit);

    let response: PagedResponse<Schedule> = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list schedules")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
        timezone: &'a str,
        enabled: bool,
        owner_team: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .iter()
        .map(|schedule| Row {
            id: &schedule.id,
            name: &schedule.name,
            description: schedule.description.as_deref().unwrap_or(""),
            timezone: schedule.timezone.as_deref().unwrap_or(""),
            enabled: schedule.enabled.unwrap_or(true),
            owner_team: schedule
                .owner_team
                .as_ref()
                .and_then(|t| t.name.as_deref())
                .unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No schedules found");
        println!("No schedules found");
        return Ok(());
    }

    tracing::info!("Found {} schedules", rows.len());
    ctx.renderer.render(&rows)
}

/// Get schedule details.
pub async fn get_schedule(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("schedules/{}", identifier);
    tracing::debug!("Fetching schedule {}", identifier);

    let response: ApiResponse<Schedule> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch schedule {}", identifier))?;

    let schedule = response.data;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
        timezone: &'a str,
        enabled: bool,
        owner_team: &'a str,
    }

    let view = View {
        id: &schedule.id,
        name: &schedule.name,
        description: schedule.description.as_deref().unwrap_or(""),
        timezone: schedule.timezone.as_deref().unwrap_or(""),
        enabled: schedule.enabled.unwrap_or(true),
        owner_team: schedule
            .owner_team
            .as_ref()
            .and_then(|t| t.name.as_deref())
            .unwrap_or(""),
    };

    tracing::info!("Retrieved schedule {}", identifier);
    ctx.renderer.render(&view)
}

/// Create a new schedule.
pub async fn create_schedule(
    ctx: &OpsgenieContext<'_>,
    name: String,
    description: Option<String>,
    timezone: Option<String>,
    team_id: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateScheduleBody {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timezone: Option<String>,
        #[serde(rename = "ownerTeam", skip_serializing_if = "Option::is_none")]
        owner_team: Option<TeamRef>,
        enabled: bool,
    }

    #[derive(Serialize)]
    struct TeamRef {
        id: String,
    }

    let body = CreateScheduleBody {
        name: name.clone(),
        description,
        timezone,
        owner_team: team_id.map(|id| TeamRef { id }),
        enabled: true,
    };

    tracing::debug!("Creating schedule: {}", name);

    #[derive(serde::Deserialize)]
    struct CreateResult {
        id: String,
        name: String,
    }

    let response: ApiResponse<CreateResult> = ctx
        .client
        .post("schedules", &body)
        .await
        .context("Failed to create schedule")?;

    tracing::info!(
        "Created schedule {} (id: {})",
        response.data.name,
        response.data.id
    );
    println!(
        "Created schedule: {} (id: {})",
        response.data.name, response.data.id
    );
    Ok(())
}

/// Delete a schedule.
pub async fn delete_schedule(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("schedules/{}", identifier);

    tracing::debug!("Deleting schedule {}", identifier);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to delete schedule {}", identifier))?;

    tracing::info!("Deleted schedule {}", identifier);
    println!("Successfully deleted schedule {}", identifier);
    Ok(())
}

/// Enable a schedule.
pub async fn enable_schedule(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("schedules/{}/enable", identifier);

    tracing::debug!("Enabling schedule {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to enable schedule {}", identifier))?;

    tracing::info!("Enabled schedule {}", identifier);
    println!("Successfully enabled schedule {}", identifier);
    Ok(())
}

/// Disable a schedule.
pub async fn disable_schedule(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("schedules/{}/disable", identifier);

    tracing::debug!("Disabling schedule {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to disable schedule {}", identifier))?;

    tracing::info!("Disabled schedule {}", identifier);
    println!("Successfully disabled schedule {}", identifier);
    Ok(())
}

/// Get who is on-call for a schedule.
pub async fn get_on_call(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    date: Option<&str>,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct OnCallData {
        #[serde(rename = "onCallParticipants")]
        on_call_participants: Vec<OnCallParticipant>,
    }

    let mut path = format!("schedules/{}/on-calls", identifier);
    if let Some(d) = date {
        path = format!("{}?date={}", path, d);
    }

    tracing::debug!("Getting on-call for schedule {}", identifier);

    let response: ApiResponse<OnCallData> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to get on-call for schedule {}", identifier))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        participant_type: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .on_call_participants
        .iter()
        .map(|p| Row {
            id: p.id.as_deref().unwrap_or(""),
            name: p.name.as_deref().unwrap_or(""),
            participant_type: p.participant_type.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No one is on-call for schedule {}", identifier);
        println!("No one is on-call");
        return Ok(());
    }

    tracing::info!("Found {} on-call participants", rows.len());
    ctx.renderer.render(&rows)
}

/// Get schedule timeline.
pub async fn get_timeline(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    interval: Option<i32>,
    interval_unit: Option<&str>,
    date: Option<&str>,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct TimelineData {
        #[serde(rename = "finalTimeline")]
        final_timeline: FinalTimeline,
    }

    #[derive(serde::Deserialize)]
    struct FinalTimeline {
        rotations: Vec<TimelineRotation>,
    }

    #[allow(dead_code)]
    #[derive(serde::Deserialize)]
    struct TimelineRotation {
        id: String,
        name: String,
        periods: Vec<TimelinePeriod>,
    }

    #[derive(serde::Deserialize)]
    struct TimelinePeriod {
        #[serde(rename = "startDate")]
        start_date: String,
        #[serde(rename = "endDate")]
        end_date: String,
        recipient: Option<TimelineRecipient>,
    }

    #[allow(dead_code)]
    #[derive(serde::Deserialize)]
    struct TimelineRecipient {
        id: Option<String>,
        name: Option<String>,
        #[serde(rename = "type")]
        recipient_type: Option<String>,
    }

    let mut params = vec![];
    if let Some(i) = interval {
        params.push(format!("interval={}", i));
    }
    if let Some(u) = interval_unit {
        params.push(format!("intervalUnit={}", u));
    }
    if let Some(d) = date {
        params.push(format!("date={}", d));
    }

    let path = if params.is_empty() {
        format!("schedules/{}/timeline", identifier)
    } else {
        format!("schedules/{}/timeline?{}", identifier, params.join("&"))
    };

    tracing::debug!("Getting timeline for schedule {}", identifier);

    let response: ApiResponse<TimelineData> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to get timeline for schedule {}", identifier))?;

    #[derive(Serialize)]
    struct Row {
        rotation_name: String,
        start_date: String,
        end_date: String,
        recipient_name: String,
        recipient_type: String,
    }

    let mut rows = Vec::new();
    for rotation in response.data.final_timeline.rotations {
        for period in rotation.periods {
            rows.push(Row {
                rotation_name: rotation.name.clone(),
                start_date: period.start_date,
                end_date: period.end_date,
                recipient_name: period
                    .recipient
                    .as_ref()
                    .and_then(|r| r.name.clone())
                    .unwrap_or_default(),
                recipient_type: period
                    .recipient
                    .as_ref()
                    .and_then(|r| r.recipient_type.clone())
                    .unwrap_or_default(),
            });
        }
    }

    if rows.is_empty() {
        tracing::info!("No timeline data for schedule {}", identifier);
        println!("No timeline data found");
        return Ok(());
    }

    tracing::info!("Found {} timeline periods", rows.len());
    ctx.renderer.render(&rows)
}

/// Export schedule to iCal format.
pub async fn export_ical(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("schedules/{}.ics", identifier);

    tracing::debug!("Exporting schedule {} to iCal", identifier);

    let ical_content = ctx
        .client
        .get_text(&path)
        .await
        .with_context(|| format!("Failed to export schedule {} to iCal", identifier))?;

    tracing::info!("Exported schedule {} to iCal format", identifier);
    println!("{}", ical_content);
    Ok(())
}
