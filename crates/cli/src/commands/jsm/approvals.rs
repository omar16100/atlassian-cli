use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::{Approval, JsmContext};

/// List approvals for a request.
pub async fn list_approvals(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct ApprovalList {
        values: Vec<Approval>,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/approval");
    tracing::debug!("Listing approvals for request {}", key);

    let response: ApprovalList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list approvals for request {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        decision: &'a str,
        can_answer: bool,
        approvers: String,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|a| Row {
            id: &a.id,
            name: &a.name,
            decision: a.final_decision.as_deref().unwrap_or("pending"),
            can_answer: a.can_answer_approval,
            approvers: a
                .approvers
                .iter()
                .filter_map(|ap| ap.approver.as_ref().and_then(|u| u.display_name.as_ref()))
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No approvals for request {}", key);
    }

    tracing::info!("Found {} approvals for request {}", rows.len(), key);
    ctx.renderer
        .render_list_or_empty(&rows, "No approvals found")
}

/// Get approval details.
pub async fn get_approval(ctx: &JsmContext<'_>, key: &str, approval_id: i64) -> Result<()> {
    let path = format!("/rest/servicedeskapi/request/{key}/approval/{approval_id}");
    tracing::debug!("Fetching approval {} for request {}", approval_id, key);

    let approval: Approval =
        ctx.client.get(&path).await.with_context(|| {
            format!("Failed to fetch approval {} for request {key}", approval_id)
        })?;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
        decision: &'a str,
        can_answer: bool,
        approvers: Vec<ApproverView<'a>>,
    }

    #[derive(Serialize)]
    struct ApproverView<'a> {
        name: &'a str,
        decision: &'a str,
    }

    let view = View {
        id: &approval.id,
        name: &approval.name,
        decision: approval.final_decision.as_deref().unwrap_or("pending"),
        can_answer: approval.can_answer_approval,
        approvers: approval
            .approvers
            .iter()
            .map(|a| ApproverView {
                name: a
                    .approver
                    .as_ref()
                    .and_then(|u| u.display_name.as_deref())
                    .unwrap_or(""),
                decision: a.approver_decision.as_deref().unwrap_or("pending"),
            })
            .collect(),
    };

    tracing::info!("Retrieved approval {}", approval.name);
    ctx.renderer.render(&view)
}

/// Answer an approval (approve or decline).
pub async fn answer_approval(
    ctx: &JsmContext<'_>,
    key: &str,
    approval_id: i64,
    approve: bool,
) -> Result<()> {
    #[derive(Serialize)]
    struct ApprovalAnswer {
        decision: String,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/approval/{approval_id}");
    let decision = if approve { "approve" } else { "decline" };
    let body = ApprovalAnswer {
        decision: decision.to_string(),
    };

    tracing::debug!(
        "Answering approval {} for request {} with decision: {}",
        approval_id,
        key,
        decision
    );
    ctx.client
        .post::<(), _>(&path, &body)
        .await
        .with_context(|| {
            format!(
                "Failed to answer approval {} for request {key}",
                approval_id
            )
        })?;

    tracing::info!(
        "Answered approval {} for request {} with decision: {}",
        approval_id,
        key,
        decision
    );
    println!("Successfully {}d request {}", decision, key);
    Ok(())
}
