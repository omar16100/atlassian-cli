use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::{JsmContext, SlaInformation};

/// List SLAs for a request.
pub async fn list_slas(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct SlaList {
        values: Vec<SlaInformation>,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/sla");
    tracing::debug!("Listing SLAs for request {}", key);

    let response: SlaList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list SLAs for request {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        breached: bool,
        status: &'static str,
        remaining: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|sla| {
            let (breached, status, remaining) = if let Some(ref ongoing) = sla.ongoing_cycle {
                (
                    ongoing.breached,
                    if ongoing.breached {
                        "BREACHED"
                    } else {
                        "ACTIVE"
                    },
                    ongoing
                        .remaining_time
                        .as_ref()
                        .map(|r| r.friendly.as_str())
                        .unwrap_or("N/A"),
                )
            } else {
                let last_cycle = sla.completed_cycles.last();
                (
                    last_cycle.map(|c| c.breached).unwrap_or(false),
                    "COMPLETED",
                    "N/A",
                )
            };
            Row {
                id: &sla.id,
                name: &sla.name,
                breached,
                status,
                remaining,
            }
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No SLAs for request {}", key);
    }

    tracing::info!("Found {} SLAs for request {}", rows.len(), key);
    ctx.renderer.render_list_or_empty(&rows, "No SLAs found")
}

/// Get specific SLA details.
pub async fn get_sla(ctx: &JsmContext<'_>, key: &str, sla_id: &str) -> Result<()> {
    let path = format!("/rest/servicedeskapi/request/{key}/sla/{sla_id}");
    tracing::debug!("Fetching SLA {} for request {}", sla_id, key);

    let sla: SlaInformation = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch SLA {} for request {key}", sla_id))?;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
        completed_cycles: usize,
        ongoing: Option<OngoingView<'a>>,
    }

    #[derive(Serialize)]
    struct OngoingView<'a> {
        breached: bool,
        started: &'a str,
        goal: &'a str,
        elapsed: &'a str,
        remaining: &'a str,
    }

    let view = View {
        id: &sla.id,
        name: &sla.name,
        completed_cycles: sla.completed_cycles.len(),
        ongoing: sla.ongoing_cycle.as_ref().map(|c| OngoingView {
            breached: c.breached,
            started: c
                .start_time
                .as_ref()
                .map(|d| d.iso8601.as_str())
                .unwrap_or(""),
            goal: c
                .goal_duration
                .as_ref()
                .map(|d| d.friendly.as_str())
                .unwrap_or(""),
            elapsed: c
                .elapsed_time
                .as_ref()
                .map(|d| d.friendly.as_str())
                .unwrap_or(""),
            remaining: c
                .remaining_time
                .as_ref()
                .map(|d| d.friendly.as_str())
                .unwrap_or(""),
        }),
    };

    tracing::info!("Retrieved SLA {} for request {}", sla.name, key);
    ctx.renderer.render(&view)
}
