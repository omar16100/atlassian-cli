use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{ApiResponse, OpsgenieContext, PagedResponse, Service};

/// List all services.
pub async fn list_services(ctx: &OpsgenieContext<'_>, limit: usize) -> Result<()> {
    let path = format!("services?limit={}", limit.min(100));
    tracing::debug!("Listing services with limit {}", limit);

    let response: PagedResponse<Service> = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list services")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
        team_id: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .iter()
        .map(|svc| Row {
            id: &svc.id,
            name: &svc.name,
            description: svc.description.as_deref().unwrap_or(""),
            team_id: svc.team_id.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No services found");
    }

    tracing::info!("Found {} services", rows.len());
    ctx.renderer
        .render_list_or_empty(&rows, "No services found")
}

/// Get service details.
pub async fn get_service(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("services/{}", identifier);
    tracing::debug!("Fetching service {}", identifier);

    let response: ApiResponse<Service> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch service {}", identifier))?;

    let svc = response.data;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
        team_id: &'a str,
    }

    let view = View {
        id: &svc.id,
        name: &svc.name,
        description: svc.description.as_deref().unwrap_or(""),
        team_id: svc.team_id.as_deref().unwrap_or(""),
    };

    tracing::info!("Retrieved service {}", identifier);
    ctx.renderer.render(&view)
}

/// Create a new service.
pub async fn create_service(
    ctx: &OpsgenieContext<'_>,
    name: String,
    team_id: String,
    description: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateServiceBody {
        name: String,
        #[serde(rename = "teamId")]
        team_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    }

    let body = CreateServiceBody {
        name: name.clone(),
        team_id,
        description,
    };

    tracing::debug!("Creating service: {}", name);

    #[derive(serde::Deserialize)]
    struct CreateResult {
        id: String,
        name: String,
    }

    let response: ApiResponse<CreateResult> = ctx
        .client
        .post("services", &body)
        .await
        .context("Failed to create service")?;

    tracing::info!(
        "Created service {} (id: {})",
        response.data.name,
        response.data.id
    );
    println!(
        "Created service: {} (id: {})",
        response.data.name, response.data.id
    );
    Ok(())
}

/// Delete a service.
pub async fn delete_service(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("services/{}", identifier);

    tracing::debug!("Deleting service {}", identifier);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to delete service {}", identifier))?;

    tracing::info!("Deleted service {}", identifier);
    println!("Successfully deleted service {}", identifier);
    Ok(())
}
