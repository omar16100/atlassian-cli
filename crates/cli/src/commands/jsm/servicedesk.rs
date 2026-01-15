use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::{JsmContext, Organization, User};

/// Service desk from API response.
#[derive(Deserialize)]
struct ServiceDesk {
    id: i64,
    #[serde(rename = "projectId", default)]
    project_id: Option<String>,
    #[serde(rename = "projectKey", default)]
    project_key: Option<String>,
    #[serde(rename = "projectName", default)]
    project_name: Option<String>,
    name: String,
    #[serde(rename = "_links", default)]
    links: Option<Links>,
}

#[derive(Deserialize, Default)]
struct Links {
    #[serde(default)]
    portal: Option<String>,
}

/// List service desks.
pub async fn list_service_desks(ctx: &JsmContext<'_>, limit: usize) -> Result<()> {
    #[derive(Deserialize)]
    struct DeskList {
        values: Vec<ServiceDesk>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/servicedesk?{query}");

    tracing::debug!("Listing service desks with limit {}", limit);
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
        tracing::info!("No service desks found");
        println!("No service desks found");
        return Ok(());
    }

    tracing::info!("Found {} service desks", rows.len());
    ctx.renderer.render(&rows)
}

/// Get service desk details.
pub async fn get_service_desk(ctx: &JsmContext<'_>, id: i64) -> Result<()> {
    let path = format!("/rest/servicedeskapi/servicedesk/{id}");
    tracing::debug!("Fetching service desk {}", id);

    let desk: ServiceDesk = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch service desk {}", id))?;

    #[derive(Serialize)]
    struct View<'a> {
        id: i64,
        name: &'a str,
        project_id: &'a str,
        project_key: &'a str,
        project_name: &'a str,
        portal_url: &'a str,
    }

    let view = View {
        id: desk.id,
        name: desk.name.as_str(),
        project_id: desk.project_id.as_deref().unwrap_or(""),
        project_key: desk.project_key.as_deref().unwrap_or(""),
        project_name: desk.project_name.as_deref().unwrap_or(""),
        portal_url: desk
            .links
            .as_ref()
            .and_then(|l| l.portal.as_deref())
            .unwrap_or(""),
    };

    tracing::info!("Retrieved service desk {} ({})", desk.name, id);
    ctx.renderer.render(&view)
}

/// List customers of a service desk.
pub async fn list_customers(ctx: &JsmContext<'_>, servicedesk_id: i64, limit: usize) -> Result<()> {
    #[derive(Deserialize)]
    struct CustomerList {
        values: Vec<User>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/customer?{query}");

    tracing::debug!("Listing customers for service desk {}", servicedesk_id);
    let response: CustomerList = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to list customers for service desk {}",
            servicedesk_id
        )
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        account_id: &'a str,
        display_name: &'a str,
        email: &'a str,
        active: bool,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|user| Row {
            account_id: &user.account_id,
            display_name: user.display_name.as_deref().unwrap_or(""),
            email: user.email_address.as_deref().unwrap_or(""),
            active: user.active,
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No customers found for service desk {}", servicedesk_id);
        println!("No customers found");
        return Ok(());
    }

    tracing::info!("Found {} customers", rows.len());
    ctx.renderer.render(&rows)
}

/// Add customer to service desk.
pub async fn add_customer(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    account_ids: Vec<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct AddCustomerRequest {
        #[serde(rename = "accountIds")]
        account_ids: Vec<String>,
    }

    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/customer");
    let body = AddCustomerRequest {
        account_ids: account_ids.clone(),
    };

    tracing::debug!(
        "Adding {} customers to service desk {}",
        account_ids.len(),
        servicedesk_id
    );
    ctx.client
        .post::<(), _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add customers to service desk {}", servicedesk_id))?;

    tracing::info!(
        "Added {} customers to service desk {}",
        account_ids.len(),
        servicedesk_id
    );
    println!(
        "Successfully added {} customer(s) to service desk {}",
        account_ids.len(),
        servicedesk_id
    );
    Ok(())
}

/// Remove customer from service desk.
pub async fn remove_customer(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    account_ids: Vec<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct RemoveCustomerRequest {
        #[serde(rename = "accountIds")]
        account_ids: Vec<String>,
    }

    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/customer");
    let body = RemoveCustomerRequest {
        account_ids: account_ids.clone(),
    };

    tracing::debug!(
        "Removing {} customers from service desk {}",
        account_ids.len(),
        servicedesk_id
    );
    ctx.client
        .delete_with_body::<(), _>(&path, &body)
        .await
        .with_context(|| {
            format!(
                "Failed to remove customers from service desk {}",
                servicedesk_id
            )
        })?;

    tracing::info!(
        "Removed {} customers from service desk {}",
        account_ids.len(),
        servicedesk_id
    );
    println!(
        "Successfully removed {} customer(s) from service desk {}",
        account_ids.len(),
        servicedesk_id
    );
    Ok(())
}

/// List organizations of a service desk.
pub async fn list_organizations(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    limit: usize,
) -> Result<()> {
    #[derive(Deserialize)]
    struct OrgList {
        values: Vec<Organization>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/organization?{query}");

    tracing::debug!("Listing organizations for service desk {}", servicedesk_id);
    let response: OrgList = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to list organizations for service desk {}",
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
        .map(|org| Row {
            id: &org.id,
            name: &org.name,
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No organizations found for service desk {}", servicedesk_id);
        println!("No organizations found");
        return Ok(());
    }

    tracing::info!("Found {} organizations", rows.len());
    ctx.renderer.render(&rows)
}

/// Add organization to service desk.
pub async fn add_organization(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    organization_id: i64,
) -> Result<()> {
    #[derive(Serialize)]
    struct AddOrgRequest {
        #[serde(rename = "organizationId")]
        organization_id: i64,
    }

    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/organization");
    let body = AddOrgRequest { organization_id };

    tracing::debug!(
        "Adding organization {} to service desk {}",
        organization_id,
        servicedesk_id
    );
    ctx.client
        .post::<(), _>(&path, &body)
        .await
        .with_context(|| {
            format!(
                "Failed to add organization {} to service desk {}",
                organization_id, servicedesk_id
            )
        })?;

    tracing::info!(
        "Added organization {} to service desk {}",
        organization_id,
        servicedesk_id
    );
    println!(
        "Successfully added organization {} to service desk {}",
        organization_id, servicedesk_id
    );
    Ok(())
}

/// Remove organization from service desk.
pub async fn remove_organization(
    ctx: &JsmContext<'_>,
    servicedesk_id: i64,
    organization_id: i64,
) -> Result<()> {
    #[derive(Serialize)]
    struct RemoveOrgRequest {
        #[serde(rename = "organizationId")]
        organization_id: i64,
    }

    let path = format!("/rest/servicedeskapi/servicedesk/{servicedesk_id}/organization");
    let body = RemoveOrgRequest { organization_id };

    tracing::debug!(
        "Removing organization {} from service desk {}",
        organization_id,
        servicedesk_id
    );
    ctx.client
        .delete_with_body::<(), _>(&path, &body)
        .await
        .with_context(|| {
            format!(
                "Failed to remove organization {} from service desk {}",
                organization_id, servicedesk_id
            )
        })?;

    tracing::info!(
        "Removed organization {} from service desk {}",
        organization_id,
        servicedesk_id
    );
    println!(
        "Successfully removed organization {} from service desk {}",
        organization_id, servicedesk_id
    );
    Ok(())
}
