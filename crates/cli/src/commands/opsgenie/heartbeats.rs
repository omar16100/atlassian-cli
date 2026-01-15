use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{ApiResponse, Heartbeat, OpsgenieContext};

/// List all heartbeats.
pub async fn list_heartbeats(ctx: &OpsgenieContext<'_>) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct HeartbeatsResponse {
        data: HeartbeatsData,
    }

    #[derive(serde::Deserialize)]
    struct HeartbeatsData {
        heartbeats: Vec<Heartbeat>,
    }

    tracing::debug!("Listing heartbeats");

    let response: HeartbeatsResponse = ctx
        .client
        .get("heartbeats")
        .await
        .context("Failed to list heartbeats")?;

    #[derive(Serialize)]
    struct Row<'a> {
        name: &'a str,
        description: &'a str,
        interval: i32,
        interval_unit: &'a str,
        enabled: bool,
        expired: bool,
        owner_team: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .heartbeats
        .iter()
        .map(|hb| Row {
            name: &hb.name,
            description: hb.description.as_deref().unwrap_or(""),
            interval: hb.interval.unwrap_or(0),
            interval_unit: hb.interval_unit.as_deref().unwrap_or(""),
            enabled: hb.enabled.unwrap_or(true),
            expired: hb.expired.unwrap_or(false),
            owner_team: hb
                .owner_team
                .as_ref()
                .and_then(|t| t.name.as_deref())
                .unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No heartbeats found");
        println!("No heartbeats found");
        return Ok(());
    }

    tracing::info!("Found {} heartbeats", rows.len());
    ctx.renderer.render(&rows)
}

/// Get heartbeat details.
pub async fn get_heartbeat(ctx: &OpsgenieContext<'_>, name: &str) -> Result<()> {
    let path = format!("heartbeats/{}", name);
    tracing::debug!("Fetching heartbeat {}", name);

    let response: ApiResponse<Heartbeat> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch heartbeat {}", name))?;

    let hb = response.data;

    #[derive(Serialize)]
    struct View<'a> {
        name: &'a str,
        description: &'a str,
        interval: i32,
        interval_unit: &'a str,
        enabled: bool,
        expired: bool,
        owner_team: &'a str,
    }

    let view = View {
        name: &hb.name,
        description: hb.description.as_deref().unwrap_or(""),
        interval: hb.interval.unwrap_or(0),
        interval_unit: hb.interval_unit.as_deref().unwrap_or(""),
        enabled: hb.enabled.unwrap_or(true),
        expired: hb.expired.unwrap_or(false),
        owner_team: hb
            .owner_team
            .as_ref()
            .and_then(|t| t.name.as_deref())
            .unwrap_or(""),
    };

    tracing::info!("Retrieved heartbeat {}", name);
    ctx.renderer.render(&view)
}

/// Create a new heartbeat.
pub async fn create_heartbeat(
    ctx: &OpsgenieContext<'_>,
    name: String,
    description: Option<String>,
    interval: i32,
    interval_unit: String,
    team_id: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateHeartbeatBody {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        interval: i32,
        #[serde(rename = "intervalUnit")]
        interval_unit: String,
        enabled: bool,
        #[serde(rename = "ownerTeam", skip_serializing_if = "Option::is_none")]
        owner_team: Option<TeamRef>,
    }

    #[derive(Serialize)]
    struct TeamRef {
        id: String,
    }

    let body = CreateHeartbeatBody {
        name: name.clone(),
        description,
        interval,
        interval_unit,
        enabled: true,
        owner_team: team_id.map(|id| TeamRef { id }),
    };

    tracing::debug!("Creating heartbeat: {}", name);

    #[derive(serde::Deserialize)]
    struct CreateResult {
        name: String,
    }

    let response: ApiResponse<CreateResult> = ctx
        .client
        .post("heartbeats", &body)
        .await
        .context("Failed to create heartbeat")?;

    tracing::info!("Created heartbeat {}", response.data.name);
    println!("Created heartbeat: {}", response.data.name);
    Ok(())
}

/// Delete a heartbeat.
pub async fn delete_heartbeat(ctx: &OpsgenieContext<'_>, name: &str) -> Result<()> {
    let path = format!("heartbeats/{}", name);

    tracing::debug!("Deleting heartbeat {}", name);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to delete heartbeat {}", name))?;

    tracing::info!("Deleted heartbeat {}", name);
    println!("Successfully deleted heartbeat {}", name);
    Ok(())
}

/// Enable a heartbeat.
pub async fn enable_heartbeat(ctx: &OpsgenieContext<'_>, name: &str) -> Result<()> {
    let path = format!("heartbeats/{}/enable", name);

    tracing::debug!("Enabling heartbeat {}", name);
    ctx.client
        .post::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to enable heartbeat {}", name))?;

    tracing::info!("Enabled heartbeat {}", name);
    println!("Successfully enabled heartbeat {}", name);
    Ok(())
}

/// Disable a heartbeat.
pub async fn disable_heartbeat(ctx: &OpsgenieContext<'_>, name: &str) -> Result<()> {
    let path = format!("heartbeats/{}/disable", name);

    tracing::debug!("Disabling heartbeat {}", name);
    ctx.client
        .post::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to disable heartbeat {}", name))?;

    tracing::info!("Disabled heartbeat {}", name);
    println!("Successfully disabled heartbeat {}", name);
    Ok(())
}

/// Send a ping to a heartbeat.
pub async fn ping_heartbeat(ctx: &OpsgenieContext<'_>, name: &str) -> Result<()> {
    let path = format!("heartbeats/{}/ping", name);

    tracing::debug!("Pinging heartbeat {}", name);
    ctx.client
        .get::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to ping heartbeat {}", name))?;

    tracing::info!("Pinged heartbeat {}", name);
    println!("Successfully pinged heartbeat {}", name);
    Ok(())
}
