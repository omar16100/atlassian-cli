use anyhow::{Context, Result};
use serde::Serialize;
use url::form_urlencoded;

use super::utils::{Alert, ApiResponse, OpsgenieContext, PagedResponse};

/// List alerts with optional filters.
pub async fn list_alerts(
    ctx: &OpsgenieContext<'_>,
    query: Option<&str>,
    limit: usize,
    status: Option<&str>,
) -> Result<()> {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("limit", &limit.min(100).to_string());

    // Combine query and status into single query param
    let mut query_parts = Vec::new();
    if let Some(q) = query {
        query_parts.push(q.to_string());
    }
    if let Some(s) = status {
        query_parts.push(format!("status:{}", s));
    }
    if !query_parts.is_empty() {
        serializer.append_pair("query", &query_parts.join(" AND "));
    }

    let path = format!("alerts?{}", serializer.finish());
    tracing::debug!("Listing alerts with limit {}", limit);

    let response: PagedResponse<Alert> = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list alerts")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        tiny_id: &'a str,
        message: &'a str,
        status: &'a str,
        priority: &'a str,
        acknowledged: bool,
        created_at: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .iter()
        .map(|alert| Row {
            id: &alert.id,
            tiny_id: alert.tiny_id.as_deref().unwrap_or(""),
            message: &alert.message,
            status: alert.status.as_deref().unwrap_or(""),
            priority: alert.priority.as_deref().unwrap_or(""),
            acknowledged: alert.acknowledged.unwrap_or(false),
            created_at: alert.created_at.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No alerts found");
    } else {
        tracing::info!("Found {} alerts", rows.len());
    }
    ctx.renderer.render(&rows)
}

/// Get alert details by ID or alias.
pub async fn get_alert(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("alerts/{}", identifier);
    tracing::debug!("Fetching alert {}", identifier);

    let response: ApiResponse<Alert> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch alert {}", identifier))?;

    let alert = response.data;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        tiny_id: &'a str,
        alias: &'a str,
        message: &'a str,
        status: &'a str,
        priority: &'a str,
        acknowledged: bool,
        snoozed: bool,
        source: &'a str,
        owner: &'a str,
        tags: String,
        created_at: &'a str,
        updated_at: &'a str,
    }

    let view = View {
        id: &alert.id,
        tiny_id: alert.tiny_id.as_deref().unwrap_or(""),
        alias: alert.alias.as_deref().unwrap_or(""),
        message: &alert.message,
        status: alert.status.as_deref().unwrap_or(""),
        priority: alert.priority.as_deref().unwrap_or(""),
        acknowledged: alert.acknowledged.unwrap_or(false),
        snoozed: alert.snoozed.unwrap_or(false),
        source: alert.source.as_deref().unwrap_or(""),
        owner: alert.owner.as_deref().unwrap_or(""),
        tags: alert
            .tags
            .as_ref()
            .map(|t| t.join(", "))
            .unwrap_or_default(),
        created_at: alert.created_at.as_deref().unwrap_or(""),
        updated_at: alert.updated_at.as_deref().unwrap_or(""),
    };

    tracing::info!("Retrieved alert {}", identifier);
    ctx.renderer.render(&view)
}

/// Create a new alert.
pub async fn create_alert(
    ctx: &OpsgenieContext<'_>,
    message: String,
    alias: Option<String>,
    description: Option<String>,
    priority: Option<String>,
    source: Option<String>,
    tags: Vec<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateAlertBody {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        alias: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        priority: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
    }

    let body = CreateAlertBody {
        message: message.clone(),
        alias,
        description,
        priority,
        source,
        tags,
    };

    tracing::debug!("Creating alert: {}", message);

    #[allow(dead_code)]
    #[derive(serde::Deserialize)]
    struct CreateResult {
        #[serde(rename = "alertId")]
        alert_id: Option<String>,
        alias: Option<String>,
    }

    let response: ApiResponse<CreateResult> = ctx
        .client
        .post("alerts", &body)
        .await
        .context("Failed to create alert")?;

    let id = response.data.alert_id.unwrap_or_default();
    tracing::info!("Created alert {}", id);
    println!("Created alert: {} (id: {})", message, id);
    Ok(())
}

/// Close an alert.
pub async fn close_alert(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CloseBody {
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    let path = format!("alerts/{}/close", identifier);
    let body = CloseBody { note };

    tracing::debug!("Closing alert {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to close alert {}", identifier))?;

    tracing::info!("Closed alert {}", identifier);
    println!("Successfully closed alert {}", identifier);
    Ok(())
}

/// Acknowledge an alert.
pub async fn acknowledge_alert(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct AckBody {
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    let path = format!("alerts/{}/acknowledge", identifier);
    let body = AckBody { note };

    tracing::debug!("Acknowledging alert {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to acknowledge alert {}", identifier))?;

    tracing::info!("Acknowledged alert {}", identifier);
    println!("Successfully acknowledged alert {}", identifier);
    Ok(())
}

/// Unacknowledge an alert.
pub async fn unacknowledge_alert(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct UnackBody {
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    let path = format!("alerts/{}/unacknowledge", identifier);
    let body = UnackBody { note };

    tracing::debug!("Unacknowledging alert {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to unacknowledge alert {}", identifier))?;

    tracing::info!("Unacknowledged alert {}", identifier);
    println!("Successfully unacknowledged alert {}", identifier);
    Ok(())
}

/// Snooze an alert.
pub async fn snooze_alert(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    end_time: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct SnoozeBody {
        #[serde(rename = "endTime")]
        end_time: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    let path = format!("alerts/{}/snooze", identifier);
    let body = SnoozeBody {
        end_time: end_time.to_string(),
        note,
    };

    tracing::debug!("Snoozing alert {} until {}", identifier, end_time);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to snooze alert {}", identifier))?;

    tracing::info!("Snoozed alert {} until {}", identifier, end_time);
    println!(
        "Successfully snoozed alert {} until {}",
        identifier, end_time
    );
    Ok(())
}

/// Escalate an alert to next responders.
pub async fn escalate_alert(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    escalation_id: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct EscalateBody {
        escalation: EscalationRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    #[derive(Serialize)]
    struct EscalationRef {
        id: String,
    }

    let path = format!("alerts/{}/escalate", identifier);
    let body = EscalateBody {
        escalation: EscalationRef {
            id: escalation_id.to_string(),
        },
        note,
    };

    tracing::debug!(
        "Escalating alert {} with escalation {}",
        identifier,
        escalation_id
    );
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to escalate alert {}", identifier))?;

    tracing::info!("Escalated alert {}", identifier);
    println!("Successfully escalated alert {}", identifier);
    Ok(())
}

/// Assign an alert to a user.
pub async fn assign_alert(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    owner_id: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct AssignBody {
        owner: OwnerRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    #[derive(Serialize)]
    struct OwnerRef {
        id: String,
    }

    let path = format!("alerts/{}/assign", identifier);
    let body = AssignBody {
        owner: OwnerRef {
            id: owner_id.to_string(),
        },
        note,
    };

    tracing::debug!("Assigning alert {} to {}", identifier, owner_id);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to assign alert {}", identifier))?;

    tracing::info!("Assigned alert {} to {}", identifier, owner_id);
    println!("Successfully assigned alert {} to {}", identifier, owner_id);
    Ok(())
}

/// Add note to an alert.
pub async fn add_note(ctx: &OpsgenieContext<'_>, identifier: &str, note: &str) -> Result<()> {
    #[derive(Serialize)]
    struct NoteBody<'a> {
        note: &'a str,
    }

    let path = format!("alerts/{}/notes", identifier);
    let body = NoteBody { note };

    tracing::debug!("Adding note to alert {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add note to alert {}", identifier))?;

    tracing::info!("Added note to alert {}", identifier);
    println!("Successfully added note to alert {}", identifier);
    Ok(())
}

/// Delete an alert.
pub async fn delete_alert(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("alerts/{}", identifier);

    tracing::debug!("Deleting alert {}", identifier);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to delete alert {}", identifier))?;

    tracing::info!("Deleted alert {}", identifier);
    println!("Successfully deleted alert {}", identifier);
    Ok(())
}

/// List alert recipients.
pub async fn list_recipients(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct RecipientsData {
        users: Vec<RecipientUser>,
    }

    #[derive(serde::Deserialize, Serialize)]
    struct RecipientUser {
        #[serde(default)]
        username: String,
        #[serde(default)]
        state: String,
        #[serde(rename = "stateChangedAt", default)]
        state_changed_at: String,
    }

    let path = format!("alerts/{}/recipients", identifier);
    tracing::debug!("Listing recipients for alert {}", identifier);

    let response: ApiResponse<RecipientsData> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list recipients for alert {}", identifier))?;

    if response.data.users.is_empty() {
        tracing::info!("No recipients found for alert {}", identifier);
    } else {
        tracing::info!(
            "Found {} recipients for alert {}",
            response.data.users.len(),
            identifier
        );
    }
    ctx.renderer.render(&response.data.users)
}

/// List alert logs.
pub async fn list_logs(ctx: &OpsgenieContext<'_>, identifier: &str, limit: usize) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct LogsResponse {
        data: Vec<AlertLog>,
    }

    #[derive(serde::Deserialize, Serialize)]
    struct AlertLog {
        log: String,
        #[serde(rename = "type")]
        log_type: String,
        owner: String,
        #[serde(rename = "createdAt")]
        created_at: String,
    }

    let path = format!("alerts/{}/logs?limit={}", identifier, limit.min(100));
    tracing::debug!("Listing logs for alert {}", identifier);

    let response: LogsResponse = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list logs for alert {}", identifier))?;

    if response.data.is_empty() {
        tracing::info!("No logs found for alert {}", identifier);
    } else {
        tracing::info!(
            "Found {} logs for alert {}",
            response.data.len(),
            identifier
        );
    }
    ctx.renderer.render(&response.data)
}

/// List alert notes.
pub async fn list_notes(ctx: &OpsgenieContext<'_>, identifier: &str, limit: usize) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct NotesResponse {
        data: Vec<AlertNote>,
    }

    #[derive(serde::Deserialize, Serialize)]
    struct AlertNote {
        note: String,
        owner: String,
        #[serde(rename = "createdAt")]
        created_at: String,
    }

    let path = format!("alerts/{}/notes?limit={}", identifier, limit.min(100));
    tracing::debug!("Listing notes for alert {}", identifier);

    let response: NotesResponse = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list notes for alert {}", identifier))?;

    if response.data.is_empty() {
        tracing::info!("No notes found for alert {}", identifier);
    } else {
        tracing::info!(
            "Found {} notes for alert {}",
            response.data.len(),
            identifier
        );
    }
    ctx.renderer.render(&response.data)
}
