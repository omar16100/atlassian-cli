use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{Agent, BambooContext};

/// List all agents.
pub async fn list_agents(ctx: &BambooContext<'_>) -> Result<()> {
    let path = "/rest/api/latest/agent";
    tracing::debug!("Listing agents");

    let agents: Vec<Agent> = ctx
        .client
        .get(path)
        .await
        .context("Failed to list agents")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        name: &'a str,
        agent_type: &'a str,
        active: bool,
        enabled: bool,
        busy: bool,
    }

    let rows: Vec<Row<'_>> = agents
        .iter()
        .map(|agent| Row {
            id: agent.id,
            name: &agent.name,
            agent_type: agent.agent_type.as_deref().unwrap_or(""),
            active: agent.active.unwrap_or(false),
            enabled: agent.enabled.unwrap_or(true),
            busy: agent.busy.unwrap_or(false),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No agents found");
    }

    tracing::info!("Found {} agents", rows.len());
    ctx.renderer.render_list_or_empty(&rows, "No agents found")
}

/// Get agent details.
pub async fn get_agent(ctx: &BambooContext<'_>, id: i64) -> Result<()> {
    let path = format!("/rest/api/latest/agent/{}", id);
    tracing::debug!("Fetching agent {}", id);

    let agent: Agent = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch agent {}", id))?;

    #[derive(Serialize)]
    struct View<'a> {
        id: i64,
        name: &'a str,
        agent_type: &'a str,
        active: bool,
        enabled: bool,
        busy: bool,
    }

    let view = View {
        id: agent.id,
        name: &agent.name,
        agent_type: agent.agent_type.as_deref().unwrap_or(""),
        active: agent.active.unwrap_or(false),
        enabled: agent.enabled.unwrap_or(true),
        busy: agent.busy.unwrap_or(false),
    };

    tracing::info!("Retrieved agent {}", id);
    ctx.renderer.render(&view)
}

/// Enable an agent.
pub async fn enable_agent(ctx: &BambooContext<'_>, id: i64) -> Result<()> {
    let path = format!("/rest/api/latest/agent/{}/enable", id);

    tracing::debug!("Enabling agent {}", id);
    ctx.client
        .put::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to enable agent {}", id))?;

    tracing::info!("Enabled agent {}", id);
    println!("Successfully enabled agent {}", id);
    Ok(())
}

/// Disable an agent.
pub async fn disable_agent(ctx: &BambooContext<'_>, id: i64) -> Result<()> {
    let path = format!("/rest/api/latest/agent/{}/disable", id);

    tracing::debug!("Disabling agent {}", id);
    ctx.client
        .put::<serde_json::Value, _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to disable agent {}", id))?;

    tracing::info!("Disabled agent {}", id);
    println!("Successfully disabled agent {}", id);
    Ok(())
}

/// List agent capabilities.
pub async fn list_capabilities(ctx: &BambooContext<'_>, id: i64) -> Result<()> {
    let path = format!("/rest/api/latest/agent/{}/capability", id);
    tracing::debug!("Listing capabilities for agent {}", id);

    #[derive(serde::Deserialize)]
    struct CapabilitiesResponse {
        #[serde(rename = "allCapabilities")]
        all_capabilities: Option<Vec<Capability>>,
    }

    #[derive(serde::Deserialize)]
    struct Capability {
        key: String,
        value: Option<String>,
    }

    let response: CapabilitiesResponse = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list capabilities for agent {}", id))?;

    let capabilities = response.all_capabilities.unwrap_or_default();

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        value: &'a str,
    }

    let rows: Vec<Row<'_>> = capabilities
        .iter()
        .map(|cap| Row {
            key: &cap.key,
            value: cap.value.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No capabilities found for agent {}", id);
    }

    tracing::info!("Found {} capabilities for agent {}", rows.len(), id);
    ctx.renderer
        .render_list_or_empty(&rows, "No capabilities found")
}

/// Get server info.
pub async fn get_server_info(ctx: &BambooContext<'_>) -> Result<()> {
    let path = "/rest/api/latest/info";
    tracing::debug!("Fetching server info");

    use super::utils::ServerInfo;

    let info: ServerInfo = ctx
        .client
        .get(path)
        .await
        .context("Failed to fetch server info")?;

    #[derive(Serialize)]
    struct View<'a> {
        version: &'a str,
        edition: &'a str,
        build_number: &'a str,
        build_date: &'a str,
        state: &'a str,
    }

    let view = View {
        version: info.version.as_deref().unwrap_or(""),
        edition: info.edition.as_deref().unwrap_or(""),
        build_number: info.build_number.as_deref().unwrap_or(""),
        build_date: info.build_date.as_deref().unwrap_or(""),
        state: info.state.as_deref().unwrap_or(""),
    };

    tracing::info!("Retrieved server info");
    ctx.renderer.render(&view)
}

/// List queued builds.
pub async fn list_queue(ctx: &BambooContext<'_>) -> Result<()> {
    let path = "/rest/api/latest/queue";
    tracing::debug!("Listing build queue");

    #[derive(serde::Deserialize)]
    struct QueueResponse {
        #[serde(rename = "queuedBuilds")]
        queued_builds: QueuedBuilds,
    }

    #[allow(dead_code)]
    #[derive(serde::Deserialize)]
    struct QueuedBuilds {
        size: i64,
        #[serde(rename = "queuedBuild")]
        queued_build: Option<Vec<QueuedBuild>>,
    }

    #[derive(serde::Deserialize)]
    struct QueuedBuild {
        #[serde(rename = "buildResultKey")]
        build_result_key: String,
        #[serde(rename = "planKey")]
        plan_key: String,
        #[serde(rename = "buildNumber")]
        build_number: i64,
    }

    let response: QueueResponse = ctx
        .client
        .get(path)
        .await
        .context("Failed to list build queue")?;

    let builds = response.queued_builds.queued_build.unwrap_or_default();

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        plan_key: &'a str,
        number: i64,
    }

    let rows: Vec<Row<'_>> = builds
        .iter()
        .map(|b| Row {
            key: &b.build_result_key,
            plan_key: &b.plan_key,
            number: b.build_number,
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("Build queue is empty");
    }

    tracing::info!("Found {} builds in queue", rows.len());
    ctx.renderer
        .render_list_or_empty(&rows, "Build queue is empty")
}
