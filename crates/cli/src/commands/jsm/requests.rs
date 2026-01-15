use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::{
    field_value, Comment, JsmContext, RequestField, RequestReporter, RequestStatus, Transition,
    User,
};

/// Request from API response.
#[derive(Deserialize)]
struct Request {
    #[serde(rename = "issueId")]
    #[allow(dead_code)]
    issue_id: String,
    #[serde(rename = "issueKey")]
    issue_key: String,
    #[serde(rename = "serviceDeskId")]
    service_desk_id: String,
    #[serde(rename = "createdDate", default)]
    created_date: Option<super::utils::DateDto>,
    #[serde(default)]
    reporter: Option<RequestReporter>,
    #[serde(rename = "currentStatus", default)]
    current_status: Option<RequestStatus>,
    #[serde(rename = "requestFieldValues", default)]
    request_fields: Vec<RequestField>,
}

/// List customer requests.
pub async fn list_requests(
    ctx: &JsmContext<'_>,
    servicedesk_id: Option<i64>,
    limit: usize,
) -> Result<()> {
    #[derive(Deserialize)]
    struct RequestList {
        values: Vec<Request>,
    }

    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("limit", &limit.min(50).to_string());
    if let Some(id) = servicedesk_id {
        serializer.append_pair("serviceDeskId", &id.to_string());
    }
    let path = format!("/rest/servicedeskapi/request?{}", serializer.finish());

    tracing::debug!("Listing requests with limit {}", limit);
    let response: RequestList = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list requests")?;

    #[derive(Serialize)]
    struct Row<'a> {
        issue_key: &'a str,
        service_desk_id: &'a str,
        reporter: &'a str,
        status: &'a str,
        summary: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|request| {
            let summary = field_value(&request.request_fields, "summary");
            Row {
                issue_key: request.issue_key.as_str(),
                service_desk_id: &request.service_desk_id,
                reporter: request
                    .reporter
                    .as_ref()
                    .map(|r| r.display_name.as_str())
                    .unwrap_or(""),
                status: request
                    .current_status
                    .as_ref()
                    .map(|s| s.status.as_str())
                    .unwrap_or(""),
                summary,
            }
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No requests found");
    } else {
        tracing::info!("Found {} requests", rows.len());
    }
    ctx.renderer.render(&rows)
}

/// Get request details.
pub async fn get_request(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/servicedeskapi/request/{key}");
    tracing::debug!("Fetching request {}", key);

    let request: Request = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch request {key}"))?;

    #[derive(Serialize)]
    struct View<'a> {
        issue_key: &'a str,
        service_desk_id: &'a str,
        reporter: &'a str,
        status: &'a str,
        created: &'a str,
        summary: &'a str,
        description: &'a str,
    }

    let summary = field_value(&request.request_fields, "summary");
    let description = field_value(&request.request_fields, "description");

    let view = View {
        issue_key: request.issue_key.as_str(),
        service_desk_id: &request.service_desk_id,
        reporter: request
            .reporter
            .as_ref()
            .map(|r| r.display_name.as_str())
            .unwrap_or(""),
        status: request
            .current_status
            .as_ref()
            .map(|s| s.status.as_str())
            .unwrap_or(""),
        created: request
            .created_date
            .as_ref()
            .map(|d| d.iso8601.as_str())
            .unwrap_or(""),
        summary,
        description,
    };

    tracing::info!("Retrieved request {}", key);
    ctx.renderer.render(&view)
}

/// Create a new request.
pub async fn create_request(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    request_type_id: i64,
    summary: String,
    description: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateRequestBody {
        #[serde(rename = "serviceDeskId")]
        service_desk_id: String,
        #[serde(rename = "requestTypeId")]
        request_type_id: String,
        #[serde(rename = "requestFieldValues")]
        request_field_values: RequestFieldValues,
    }

    #[derive(Serialize)]
    struct RequestFieldValues {
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    }

    let body = CreateRequestBody {
        service_desk_id: servicedesk_id.to_string(),
        request_type_id: request_type_id.to_string(),
        request_field_values: RequestFieldValues {
            summary: summary.clone(),
            description,
        },
    };

    tracing::debug!("Creating request in service desk {}", servicedesk_id);
    let response: Request = ctx
        .client
        .post("/rest/servicedeskapi/request", &body)
        .await
        .context("Failed to create request")?;

    tracing::info!(
        "Created request {} with summary: {}",
        response.issue_key,
        summary
    );
    println!("Created request: {}", response.issue_key);
    Ok(())
}

/// List available transitions for a request.
pub async fn list_transitions(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct TransitionList {
        values: Vec<Transition>,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/transition");
    tracing::debug!("Listing transitions for request {}", key);

    let response: TransitionList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list transitions for request {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|t| Row {
            id: &t.id,
            name: &t.name,
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No transitions available for request {}", key);
        println!("No transitions available");
        return Ok(());
    }

    tracing::info!("Found {} transitions for request {}", rows.len(), key);
    ctx.renderer.render(&rows)
}

/// Transition a request to a new status.
pub async fn transition_request(
    ctx: &JsmContext<'_>,
    key: &str,
    transition_id: &str,
) -> Result<()> {
    #[derive(Serialize)]
    struct TransitionBody {
        id: String,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/transition");
    let body = TransitionBody {
        id: transition_id.to_string(),
    };

    tracing::debug!(
        "Transitioning request {} with transition {}",
        key,
        transition_id
    );
    ctx.client
        .post::<(), _>(&path, &body)
        .await
        .with_context(|| format!("Failed to transition request {key}"))?;

    tracing::info!(
        "Transitioned request {} with transition {}",
        key,
        transition_id
    );
    println!("Successfully transitioned request {}", key);
    Ok(())
}

/// Get request status history.
pub async fn get_status_history(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct StatusHistory {
        values: Vec<StatusHistoryItem>,
    }

    #[derive(Deserialize)]
    struct StatusHistoryItem {
        status: RequestStatus,
        #[serde(rename = "statusDate", default)]
        status_date: Option<super::utils::DateDto>,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/status");
    tracing::debug!("Fetching status history for request {}", key);

    let response: StatusHistory = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch status history for request {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        status: &'a str,
        date: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|item| Row {
            status: &item.status.status,
            date: item
                .status_date
                .as_ref()
                .map(|d| d.iso8601.as_str())
                .unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No status history for request {}", key);
        println!("No status history");
        return Ok(());
    }

    tracing::info!("Found {} status entries for request {}", rows.len(), key);
    ctx.renderer.render(&rows)
}

/// List comments on a request.
pub async fn list_comments(ctx: &JsmContext<'_>, key: &str, limit: usize) -> Result<()> {
    #[derive(Deserialize)]
    struct CommentList {
        values: Vec<Comment>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/request/{key}/comment?{query}");

    tracing::debug!("Listing comments for request {}", key);
    let response: CommentList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list comments for request {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        author: &'a str,
        public: bool,
        body: &'a str,
        created: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|c| Row {
            id: &c.id,
            author: c
                .author
                .as_ref()
                .and_then(|a| a.display_name.as_deref())
                .unwrap_or(""),
            public: c.public,
            body: &c.body,
            created: c.created.as_ref().map(|d| d.iso8601.as_str()).unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No comments for request {}", key);
        println!("No comments found");
        return Ok(());
    }

    tracing::info!("Found {} comments for request {}", rows.len(), key);
    ctx.renderer.render(&rows)
}

/// Add comment to a request.
pub async fn add_comment(
    ctx: &JsmContext<'_>,
    key: &str,
    body: String,
    public: bool,
) -> Result<()> {
    #[derive(Serialize)]
    struct CommentBody {
        body: String,
        public: bool,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/comment");
    let comment = CommentBody { body, public };

    tracing::debug!("Adding comment to request {} (public: {})", key, public);
    let response: Comment = ctx
        .client
        .post(&path, &comment)
        .await
        .with_context(|| format!("Failed to add comment to request {key}"))?;

    tracing::info!("Added comment {} to request {}", response.id, key);
    println!("Successfully added comment to request {}", key);
    Ok(())
}

/// List participants of a request.
pub async fn list_participants(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct ParticipantList {
        values: Vec<User>,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/participant");
    tracing::debug!("Listing participants for request {}", key);

    let response: ParticipantList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list participants for request {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        account_id: &'a str,
        display_name: &'a str,
        email: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|user| Row {
            account_id: &user.account_id,
            display_name: user.display_name.as_deref().unwrap_or(""),
            email: user.email_address.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No participants for request {}", key);
        println!("No participants found");
        return Ok(());
    }

    tracing::info!("Found {} participants for request {}", rows.len(), key);
    ctx.renderer.render(&rows)
}

/// Add participant to a request.
pub async fn add_participant(
    ctx: &JsmContext<'_>,
    key: &str,
    account_ids: Vec<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct AddParticipantBody {
        #[serde(rename = "accountIds")]
        account_ids: Vec<String>,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/participant");
    let body = AddParticipantBody {
        account_ids: account_ids.clone(),
    };

    tracing::debug!(
        "Adding {} participants to request {}",
        account_ids.len(),
        key
    );
    ctx.client
        .post::<(), _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add participants to request {key}"))?;

    tracing::info!(
        "Added {} participants to request {}",
        account_ids.len(),
        key
    );
    println!(
        "Successfully added {} participant(s) to request {}",
        account_ids.len(),
        key
    );
    Ok(())
}

/// Remove participant from a request.
pub async fn remove_participant(
    ctx: &JsmContext<'_>,
    key: &str,
    account_ids: Vec<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct RemoveParticipantBody {
        #[serde(rename = "accountIds")]
        account_ids: Vec<String>,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/participant");
    let body = RemoveParticipantBody {
        account_ids: account_ids.clone(),
    };

    tracing::debug!(
        "Removing {} participants from request {}",
        account_ids.len(),
        key
    );
    ctx.client
        .delete_with_body::<(), _>(&path, &body)
        .await
        .with_context(|| format!("Failed to remove participants from request {key}"))?;

    tracing::info!(
        "Removed {} participants from request {}",
        account_ids.len(),
        key
    );
    println!(
        "Successfully removed {} participant(s) from request {}",
        account_ids.len(),
        key
    );
    Ok(())
}

/// Subscribe to request notifications.
pub async fn subscribe(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/servicedeskapi/request/{key}/notification");

    tracing::debug!("Subscribing to request {}", key);
    ctx.client
        .put::<(), _>(&path, &serde_json::json!({"subscribing": true}))
        .await
        .with_context(|| format!("Failed to subscribe to request {key}"))?;

    tracing::info!("Subscribed to request {}", key);
    println!("Successfully subscribed to request {}", key);
    Ok(())
}

/// Unsubscribe from request notifications.
pub async fn unsubscribe(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/servicedeskapi/request/{key}/notification");

    tracing::debug!("Unsubscribing from request {}", key);
    ctx.client
        .delete::<()>(&path)
        .await
        .with_context(|| format!("Failed to unsubscribe from request {key}"))?;

    tracing::info!("Unsubscribed from request {}", key);
    println!("Successfully unsubscribed from request {}", key);
    Ok(())
}
