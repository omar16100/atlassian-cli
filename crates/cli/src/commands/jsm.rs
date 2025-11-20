use anyhow::{Context, Result};
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

#[derive(Args, Debug, Clone)]
pub struct JsmArgs {
    #[command(subcommand)]
    command: JsmCommands,
}

#[derive(Subcommand, Debug, Clone)]
enum JsmCommands {
    /// Service desk operations.
    ServiceDesk {
        #[command(subcommand)]
        command: ServiceDeskCommands,
    },
    /// Customer request operations.
    Request {
        #[command(subcommand)]
        command: RequestCommands,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum ServiceDeskCommands {
    /// List service desks available to the account.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get a single service desk by ID.
    Get { id: i64 },
}

#[derive(Subcommand, Debug, Clone)]
enum RequestCommands {
    /// List requests, optionally filtered by service desk.
    List {
        #[arg(long)]
        servicedesk_id: Option<i64>,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get request details (issue key or ID).
    Get {
        #[arg(value_name = "ISSUE")]
        key: String,
    },
}

pub struct JsmContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}

#[derive(Deserialize)]
struct RequestField {
    #[serde(rename = "fieldId")]
    field_id: String,
    #[serde(rename = "label")]
    label: String,
    #[serde(rename = "value", default)]
    value: Option<String>,
}

#[derive(Deserialize)]
struct RequestReporter {
    #[serde(rename = "displayName")]
    display_name: String,
}

#[derive(Deserialize)]
struct RequestStatus {
    #[serde(rename = "status")]
    status: String,
}

pub async fn execute(args: JsmArgs, ctx: JsmContext<'_>) -> Result<()> {
    match args.command {
        JsmCommands::ServiceDesk { command } => match command {
            ServiceDeskCommands::List { limit } => list_service_desks(&ctx, limit).await,
            ServiceDeskCommands::Get { id } => get_service_desk(&ctx, id).await,
        },
        JsmCommands::Request { command } => match command {
            RequestCommands::List {
                servicedesk_id,
                limit,
            } => list_requests(&ctx, servicedesk_id, limit).await,
            RequestCommands::Get { key } => get_request(&ctx, &key).await,
        },
    }
}

async fn list_service_desks(ctx: &JsmContext<'_>, limit: usize) -> Result<()> {
    #[derive(Deserialize)]
    struct DeskList {
        values: Vec<ServiceDesk>,
    }

    #[derive(Deserialize)]
    struct ServiceDesk {
        id: i64,
        name: String,
        #[serde(default)]
        project_key: Option<String>,
        #[serde(default)]
        project_name: Option<String>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/servicedesk?{query}");

    let response: DeskList = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list service desks")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        name: &'a str,
        project_key: &'a str,
        project_name: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|desk| Row {
            id: desk.id,
            name: desk.name.as_str(),
            project_key: desk.project_key.as_deref().unwrap_or(""),
            project_name: desk.project_name.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No service desks returned.");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

async fn get_service_desk(ctx: &JsmContext<'_>, id: i64) -> Result<()> {
    #[derive(Deserialize)]
    struct ServiceDesk {
        id: i64,
        name: String,
        #[serde(default)]
        project_key: Option<String>,
        #[serde(default)]
        project_name: Option<String>,
        #[serde(default)]
        portal_name: Option<String>,
    }

    let path = format!("/rest/servicedeskapi/servicedesk/{id}");
    let desk: ServiceDesk = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch service desk {}", id))?;

    #[derive(Serialize)]
    struct View<'a> {
        id: i64,
        name: &'a str,
        project_key: &'a str,
        project_name: &'a str,
        portal_name: &'a str,
    }

    let view = View {
        id: desk.id,
        name: desk.name.as_str(),
        project_key: desk.project_key.as_deref().unwrap_or(""),
        project_name: desk.project_name.as_deref().unwrap_or(""),
        portal_name: desk.portal_name.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&view)
}

async fn list_requests(
    ctx: &JsmContext<'_>,
    servicedesk_id: Option<i64>,
    limit: usize,
) -> Result<()> {
    #[derive(Deserialize)]
    struct RequestList {
        values: Vec<Request>,
    }

    #[derive(Deserialize)]
    struct Request {
        #[serde(rename = "issueId")]
        #[allow(dead_code)]
        issue_id: String,
        #[serde(rename = "issueKey")]
        issue_key: String,
        #[serde(rename = "serviceDeskId")]
        service_desk_id: i64,
        #[serde(rename = "requestFieldValues")]
        request_fields: Vec<RequestField>,
        #[serde(default)]
        reporter: Option<RequestReporter>,
        #[serde(default)]
        current_status: Option<RequestStatus>,
    }

    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("limit", &limit.min(50).to_string());
    if let Some(id) = servicedesk_id {
        serializer.append_pair("serviceDeskId", &id.to_string());
    }
    let path = format!("/rest/servicedeskapi/request?{}", serializer.finish());

    let response: RequestList = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list requests")?;

    #[derive(Serialize)]
    struct Row<'a> {
        issue_key: &'a str,
        service_desk_id: i64,
        reporter: &'a str,
        status: &'a str,
        summary: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|request| {
            let summary = request
                .request_fields
                .iter()
                .find(|field| {
                    field.field_id == "summary" || field.label.eq_ignore_ascii_case("summary")
                })
                .and_then(|field| field.value.as_deref())
                .unwrap_or("");
            Row {
                issue_key: request.issue_key.as_str(),
                service_desk_id: request.service_desk_id,
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
        tracing::info!("No requests returned.");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

async fn get_request(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Request {
        #[serde(rename = "issueId")]
        #[allow(dead_code)]
        issue_id: String,
        #[serde(rename = "issueKey")]
        issue_key: String,
        #[serde(rename = "serviceDeskId")]
        service_desk_id: i64,
        #[serde(rename = "createdDate")]
        created_date: Option<String>,
        #[serde(rename = "reporter")]
        reporter: Option<RequestReporter>,
        #[serde(rename = "currentStatus")]
        current_status: Option<RequestStatus>,
        #[serde(rename = "requestFieldValues")]
        request_fields: Vec<RequestField>,
    }

    let path = format!("/rest/servicedeskapi/request/{key}");
    let request: Request = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch request {key}"))?;

    #[derive(Serialize)]
    struct View<'a> {
        issue_key: &'a str,
        service_desk_id: i64,
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
        service_desk_id: request.service_desk_id,
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
        created: request.created_date.as_deref().unwrap_or(""),
        summary,
        description,
    };

    ctx.renderer.render(&view)
}

fn field_value<'a>(fields: &'a [RequestField], id_or_label: &str) -> &'a str {
    fields
        .iter()
        .find_map(|field| {
            if field.field_id.eq_ignore_ascii_case(id_or_label)
                || field.label.eq_ignore_ascii_case(id_or_label)
            {
                field.value.as_deref()
            } else {
                None
            }
        })
        .unwrap_or("")
}
