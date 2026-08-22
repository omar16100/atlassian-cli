use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{ApiResponse, Escalation, OpsgenieContext, PagedResponse};

/// List all escalation policies.
pub async fn list_escalations(ctx: &OpsgenieContext<'_>, limit: usize) -> Result<()> {
    let path = format!("escalations?limit={}", limit.min(100));
    tracing::debug!("Listing escalations with limit {}", limit);

    let response: PagedResponse<Escalation> = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list escalations")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
        owner_team: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .iter()
        .map(|esc| Row {
            id: &esc.id,
            name: &esc.name,
            description: esc.description.as_deref().unwrap_or(""),
            owner_team: esc
                .owner_team
                .as_ref()
                .and_then(|t| t.name.as_deref())
                .unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No escalations found");
    }

    tracing::info!("Found {} escalations", rows.len());
    ctx.renderer
        .render_list_or_empty(&rows, "No escalations found")
}

/// Get escalation policy details.
pub async fn get_escalation(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("escalations/{}", identifier);
    tracing::debug!("Fetching escalation {}", identifier);

    let response: ApiResponse<Escalation> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch escalation {}", identifier))?;

    let esc = response.data;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
        owner_team: &'a str,
        rules_count: usize,
    }

    let view = View {
        id: &esc.id,
        name: &esc.name,
        description: esc.description.as_deref().unwrap_or(""),
        owner_team: esc
            .owner_team
            .as_ref()
            .and_then(|t| t.name.as_deref())
            .unwrap_or(""),
        rules_count: esc.rules.as_ref().map(|r| r.len()).unwrap_or(0),
    };

    tracing::info!("Retrieved escalation {}", identifier);
    ctx.renderer.render(&view)
}

/// Create a new escalation policy.
pub async fn create_escalation(
    ctx: &OpsgenieContext<'_>,
    name: String,
    description: Option<String>,
    team_id: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateEscalationBody {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(rename = "ownerTeam", skip_serializing_if = "Option::is_none")]
        owner_team: Option<TeamRef>,
        rules: Vec<RuleBody>,
    }

    #[derive(Serialize)]
    struct TeamRef {
        id: String,
    }

    #[derive(Serialize)]
    struct RuleBody {
        condition: String,
        #[serde(rename = "notifyType")]
        notify_type: String,
        delay: DelayBody,
        recipient: RecipientBody,
    }

    #[derive(Serialize)]
    struct DelayBody {
        #[serde(rename = "timeAmount")]
        time_amount: i32,
        #[serde(rename = "timeUnit")]
        time_unit: String,
    }

    #[derive(Serialize)]
    struct RecipientBody {
        #[serde(rename = "type")]
        recipient_type: String,
    }

    let body = CreateEscalationBody {
        name: name.clone(),
        description,
        owner_team: team_id.map(|id| TeamRef { id }),
        rules: vec![RuleBody {
            condition: "if-not-acked".to_string(),
            notify_type: "default".to_string(),
            delay: DelayBody {
                time_amount: 0,
                time_unit: "minutes".to_string(),
            },
            recipient: RecipientBody {
                recipient_type: "all".to_string(),
            },
        }],
    };

    tracing::debug!("Creating escalation: {}", name);

    #[derive(serde::Deserialize)]
    struct CreateResult {
        id: String,
        name: String,
    }

    let response: ApiResponse<CreateResult> = ctx
        .client
        .post("escalations", &body)
        .await
        .context("Failed to create escalation")?;

    tracing::info!(
        "Created escalation {} (id: {})",
        response.data.name,
        response.data.id
    );
    println!(
        "Created escalation: {} (id: {})",
        response.data.name, response.data.id
    );
    Ok(())
}

/// Delete an escalation policy.
pub async fn delete_escalation(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("escalations/{}", identifier);

    tracing::debug!("Deleting escalation {}", identifier);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to delete escalation {}", identifier))?;

    tracing::info!("Deleted escalation {}", identifier);
    println!("Successfully deleted escalation {}", identifier);
    Ok(())
}
