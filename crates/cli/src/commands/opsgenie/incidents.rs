use anyhow::{Context, Result};
use serde::Serialize;
use url::form_urlencoded;

use super::utils::{ApiResponse, Incident, OpsgenieContext, PagedResponse};

/// List incidents with optional filters.
pub async fn list_incidents(
    ctx: &OpsgenieContext<'_>,
    query: Option<&str>,
    limit: usize,
) -> Result<()> {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("limit", &limit.min(100).to_string());
    if let Some(q) = query {
        serializer.append_pair("query", q);
    }

    let path = format!("incidents?{}", serializer.finish());
    tracing::debug!("Listing incidents with limit {}", limit);

    let response: PagedResponse<Incident> = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list incidents")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        tiny_id: &'a str,
        message: &'a str,
        status: &'a str,
        priority: &'a str,
        created_at: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .iter()
        .map(|incident| Row {
            id: &incident.id,
            tiny_id: incident.tiny_id.as_deref().unwrap_or(""),
            message: &incident.message,
            status: incident.status.as_deref().unwrap_or(""),
            priority: incident.priority.as_deref().unwrap_or(""),
            created_at: incident.created_at.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No incidents found");
        println!("No incidents found");
        return Ok(());
    }

    tracing::info!("Found {} incidents", rows.len());
    ctx.renderer.render(&rows)
}

/// Get incident details by ID.
pub async fn get_incident(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("incidents/{}", identifier);
    tracing::debug!("Fetching incident {}", identifier);

    let response: ApiResponse<Incident> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch incident {}", identifier))?;

    let incident = response.data;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        tiny_id: &'a str,
        message: &'a str,
        status: &'a str,
        priority: &'a str,
        tags: String,
        impacted_services: String,
        created_at: &'a str,
        updated_at: &'a str,
    }

    let view = View {
        id: &incident.id,
        tiny_id: incident.tiny_id.as_deref().unwrap_or(""),
        message: &incident.message,
        status: incident.status.as_deref().unwrap_or(""),
        priority: incident.priority.as_deref().unwrap_or(""),
        tags: incident
            .tags
            .as_ref()
            .map(|t| t.join(", "))
            .unwrap_or_default(),
        impacted_services: incident
            .impacted_services
            .as_ref()
            .map(|s| s.join(", "))
            .unwrap_or_default(),
        created_at: incident.created_at.as_deref().unwrap_or(""),
        updated_at: incident.updated_at.as_deref().unwrap_or(""),
    };

    tracing::info!("Retrieved incident {}", identifier);
    ctx.renderer.render(&view)
}

/// Create a new incident.
pub async fn create_incident(
    ctx: &OpsgenieContext<'_>,
    message: String,
    description: Option<String>,
    priority: Option<String>,
    tags: Vec<String>,
    service_ids: Vec<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateIncidentBody {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        priority: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(rename = "impactedServices", skip_serializing_if = "Vec::is_empty")]
        impacted_services: Vec<String>,
    }

    let body = CreateIncidentBody {
        message: message.clone(),
        description,
        priority,
        tags,
        impacted_services: service_ids,
    };

    tracing::debug!("Creating incident: {}", message);

    #[derive(serde::Deserialize)]
    struct CreateResult {
        id: String,
    }

    let response: ApiResponse<CreateResult> = ctx
        .client
        .post("incidents/create", &body)
        .await
        .context("Failed to create incident")?;

    tracing::info!("Created incident {}", response.data.id);
    println!("Created incident: {} (id: {})", message, response.data.id);
    Ok(())
}

/// Close an incident.
pub async fn close_incident(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CloseBody {
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    let path = format!("incidents/{}/close", identifier);
    let body = CloseBody { note };

    tracing::debug!("Closing incident {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to close incident {}", identifier))?;

    tracing::info!("Closed incident {}", identifier);
    println!("Successfully closed incident {}", identifier);
    Ok(())
}

/// Resolve an incident.
pub async fn resolve_incident(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct ResolveBody {
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    let path = format!("incidents/{}/resolve", identifier);
    let body = ResolveBody { note };

    tracing::debug!("Resolving incident {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to resolve incident {}", identifier))?;

    tracing::info!("Resolved incident {}", identifier);
    println!("Successfully resolved incident {}", identifier);
    Ok(())
}

/// Reopen an incident.
pub async fn reopen_incident(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct ReopenBody {
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    let path = format!("incidents/{}/reopen", identifier);
    let body = ReopenBody { note };

    tracing::debug!("Reopening incident {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to reopen incident {}", identifier))?;

    tracing::info!("Reopened incident {}", identifier);
    println!("Successfully reopened incident {}", identifier);
    Ok(())
}

/// Add responder to an incident.
pub async fn add_responder(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    responder_id: &str,
    responder_type: &str,
    note: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct AddResponderBody {
        responder: ResponderRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    #[derive(Serialize)]
    struct ResponderRef {
        id: String,
        #[serde(rename = "type")]
        responder_type: String,
    }

    let path = format!("incidents/{}/responders", identifier);
    let body = AddResponderBody {
        responder: ResponderRef {
            id: responder_id.to_string(),
            responder_type: responder_type.to_string(),
        },
        note,
    };

    tracing::debug!(
        "Adding responder {} to incident {}",
        responder_id,
        identifier
    );
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add responder to incident {}", identifier))?;

    tracing::info!(
        "Added responder {} to incident {}",
        responder_id,
        identifier
    );
    println!(
        "Successfully added responder {} to incident {}",
        responder_id, identifier
    );
    Ok(())
}

/// Add note to an incident.
pub async fn add_note(ctx: &OpsgenieContext<'_>, identifier: &str, note: &str) -> Result<()> {
    #[derive(Serialize)]
    struct NoteBody<'a> {
        note: &'a str,
    }

    let path = format!("incidents/{}/notes", identifier);
    let body = NoteBody { note };

    tracing::debug!("Adding note to incident {}", identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add note to incident {}", identifier))?;

    tracing::info!("Added note to incident {}", identifier);
    println!("Successfully added note to incident {}", identifier);
    Ok(())
}

/// Delete an incident.
pub async fn delete_incident(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("incidents/{}", identifier);

    tracing::debug!("Deleting incident {}", identifier);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to delete incident {}", identifier))?;

    tracing::info!("Deleted incident {}", identifier);
    println!("Successfully deleted incident {}", identifier);
    Ok(())
}

/// List incident timeline entries.
pub async fn list_timeline(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    limit: usize,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct TimelineResponse {
        data: TimelineData,
    }

    #[derive(serde::Deserialize)]
    struct TimelineData {
        entries: Vec<TimelineEntry>,
    }

    #[derive(serde::Deserialize, Serialize)]
    struct TimelineEntry {
        id: String,
        #[serde(rename = "type")]
        entry_type: String,
        #[serde(rename = "eventTime")]
        event_time: String,
        description: Option<String>,
    }

    let path = format!("incidents/{}/timeline?limit={}", identifier, limit.min(100));
    tracing::debug!("Listing timeline for incident {}", identifier);

    let response: TimelineResponse = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list timeline for incident {}", identifier))?;

    if response.data.entries.is_empty() {
        println!("No timeline entries found");
        return Ok(());
    }

    tracing::info!(
        "Found {} timeline entries for incident {}",
        response.data.entries.len(),
        identifier
    );
    ctx.renderer.render(&response.data.entries)
}
