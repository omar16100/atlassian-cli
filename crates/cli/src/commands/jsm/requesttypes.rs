use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::{JsmContext, RequestType, RequestTypeField};

/// List all request types (across all service desks).
pub async fn list_all_request_types(ctx: &JsmContext<'_>, limit: usize) -> Result<()> {
    #[derive(Deserialize)]
    struct RequestTypeList {
        values: Vec<RequestType>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/requesttype?{query}");

    tracing::debug!("Listing all request types with limit {}", limit);
    let response: RequestTypeList = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list request types")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        service_desk_id: &'a str,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|rt| Row {
            id: &rt.id,
            name: &rt.name,
            service_desk_id: &rt.service_desk_id,
            description: rt.description.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No request types found");
    }

    tracing::info!("Found {} request types", rows.len());
    ctx.renderer
        .render_list_or_empty(&rows, "No request types found")
}

/// List request types for a service desk.
pub async fn list_request_types(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    limit: usize,
) -> Result<()> {
    #[derive(Deserialize)]
    struct RequestTypeList {
        values: Vec<RequestType>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/requesttype?{query}");

    tracing::debug!("Listing request types for service desk {}", servicedesk_id);
    let response: RequestTypeList = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to list request types for service desk {}",
            servicedesk_id
        )
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|rt| Row {
            id: &rt.id,
            name: &rt.name,
            description: rt.description.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No request types found for service desk {}", servicedesk_id);
    }

    tracing::info!("Found {} request types", rows.len());
    ctx.renderer
        .render_list_or_empty(&rows, "No request types found")
}

/// Get request type details.
pub async fn get_request_type(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    type_id: i64,
) -> Result<()> {
    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/requesttype/{type_id}");
    tracing::debug!(
        "Fetching request type {} for service desk {}",
        type_id,
        servicedesk_id
    );

    let rt: RequestType = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to fetch request type {} for service desk {}",
            type_id, servicedesk_id
        )
    })?;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
        service_desk_id: &'a str,
        description: &'a str,
        group_ids: &'a [String],
    }

    let view = View {
        id: &rt.id,
        name: &rt.name,
        service_desk_id: &rt.service_desk_id,
        description: rt.description.as_deref().unwrap_or(""),
        group_ids: &rt.group_ids,
    };

    tracing::info!("Retrieved request type {}", rt.name);
    ctx.renderer.render(&view)
}

/// List fields for a request type.
pub async fn list_request_type_fields(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    type_id: i64,
) -> Result<()> {
    #[derive(Deserialize)]
    struct FieldList {
        #[serde(rename = "requestTypeFields")]
        request_type_fields: Vec<RequestTypeField>,
    }

    let path =
        format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/requesttype/{type_id}/field");
    tracing::debug!(
        "Listing fields for request type {} in service desk {}",
        type_id,
        servicedesk_id
    );

    let response: FieldList = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to list fields for request type {} in service desk {}",
            type_id, servicedesk_id
        )
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        field_id: &'a str,
        name: &'a str,
        required: bool,
        valid_values: String,
    }

    let rows: Vec<Row<'_>> = response
        .request_type_fields
        .iter()
        .map(|f| Row {
            field_id: &f.field_id,
            name: &f.name,
            required: f.required,
            valid_values: f
                .valid_values
                .iter()
                .map(|v| v.label.as_deref().unwrap_or(&v.value))
                .take(3)
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(
            "No fields for request type {} in service desk {}",
            type_id,
            servicedesk_id
        );
    }

    tracing::info!("Found {} fields for request type {}", rows.len(), type_id);
    ctx.renderer.render_list_or_empty(&rows, "No fields found")
}

/// List request type groups for a service desk.
pub async fn list_request_type_groups(ctx: &JsmContext<'_>, servicedesk_id: i64) -> Result<()> {
    #[derive(Deserialize)]
    struct GroupList {
        values: Vec<RequestTypeGroup>,
    }

    #[derive(Deserialize)]
    struct RequestTypeGroup {
        id: String,
        name: String,
    }

    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/requesttypegroup");
    tracing::debug!(
        "Listing request type groups for service desk {}",
        servicedesk_id
    );

    let response: GroupList = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to list request type groups for service desk {}",
            servicedesk_id
        )
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|g| Row {
            id: &g.id,
            name: &g.name,
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No request type groups for service desk {}", servicedesk_id);
    }

    tracing::info!("Found {} request type groups", rows.len());
    ctx.renderer
        .render_list_or_empty(&rows, "No request type groups found")
}
