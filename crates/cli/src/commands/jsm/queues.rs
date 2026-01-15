use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::{JsmContext, Queue};

/// List queues for a service desk.
pub async fn list_queues(ctx: &JsmContext<'_>, servicedesk_id: i64) -> Result<()> {
    #[derive(Deserialize)]
    struct QueueList {
        values: Vec<Queue>,
    }

    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/queue");
    tracing::debug!("Listing queues for service desk {}", servicedesk_id);

    let response: QueueList =
        ctx.client.get(&path).await.with_context(|| {
            format!("Failed to list queues for service desk {}", servicedesk_id)
        })?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        issue_count: i64,
        jql: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|q| Row {
            id: &q.id,
            name: &q.name,
            issue_count: q.issue_count,
            jql: q.jql.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No queues found for service desk {}", servicedesk_id);
        println!("No queues found");
        return Ok(());
    }

    tracing::info!("Found {} queues", rows.len());
    ctx.renderer.render(&rows)
}

/// Get queue details.
pub async fn get_queue(ctx: &JsmContext<'_>, servicedesk_id: i64, queue_id: i64) -> Result<()> {
    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/queue/{queue_id}");
    tracing::debug!(
        "Fetching queue {} for service desk {}",
        queue_id,
        servicedesk_id
    );

    let queue: Queue = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to fetch queue {} for service desk {}",
            queue_id, servicedesk_id
        )
    })?;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
        issue_count: i64,
        jql: &'a str,
    }

    let view = View {
        id: &queue.id,
        name: &queue.name,
        issue_count: queue.issue_count,
        jql: queue.jql.as_deref().unwrap_or(""),
    };

    tracing::info!("Retrieved queue {}", queue.name);
    ctx.renderer.render(&view)
}

/// List issues in a queue.
pub async fn list_queue_issues(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    queue_id: i64,
    limit: usize,
) -> Result<()> {
    #[derive(Deserialize)]
    struct IssueList {
        values: Vec<QueueIssue>,
    }

    #[allow(dead_code)]
    #[derive(Deserialize)]
    struct QueueIssue {
        #[serde(rename = "issueId")]
        issue_id: String,
        #[serde(rename = "issueKey")]
        issue_key: String,
        #[serde(default)]
        fields: Option<IssueFields>,
    }

    #[derive(Deserialize)]
    struct IssueFields {
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        status: Option<StatusField>,
        #[serde(default)]
        priority: Option<PriorityField>,
    }

    #[derive(Deserialize)]
    struct StatusField {
        name: String,
    }

    #[derive(Deserialize)]
    struct PriorityField {
        name: String,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path =
        format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/queue/{queue_id}/issue?{query}");

    tracing::debug!(
        "Listing issues in queue {} for service desk {}",
        queue_id,
        servicedesk_id
    );
    let response: IssueList = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to list issues in queue {} for service desk {}",
            queue_id, servicedesk_id
        )
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        issue_key: &'a str,
        summary: &'a str,
        status: &'a str,
        priority: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|issue| Row {
            issue_key: &issue.issue_key,
            summary: issue
                .fields
                .as_ref()
                .and_then(|f| f.summary.as_deref())
                .unwrap_or(""),
            status: issue
                .fields
                .as_ref()
                .and_then(|f| f.status.as_ref())
                .map(|s| s.name.as_str())
                .unwrap_or(""),
            priority: issue
                .fields
                .as_ref()
                .and_then(|f| f.priority.as_ref())
                .map(|p| p.name.as_str())
                .unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(
            "No issues in queue {} for service desk {}",
            queue_id,
            servicedesk_id
        );
        println!("No issues in queue");
        return Ok(());
    }

    tracing::info!("Found {} issues in queue", rows.len());
    ctx.renderer.render(&rows)
}
