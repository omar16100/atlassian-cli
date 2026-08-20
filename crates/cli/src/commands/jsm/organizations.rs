use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::{JsmContext, Organization, User};

/// List all organizations.
pub async fn list_organizations(ctx: &JsmContext<'_>, limit: usize) -> Result<()> {
    #[derive(Deserialize)]
    struct OrgList {
        values: Vec<Organization>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/organization?{query}");

    tracing::debug!("Listing organizations with limit {}", limit);
    let response: OrgList = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list organizations")?;

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
        tracing::info!("No organizations found");
    }

    tracing::info!("Found {} organizations", rows.len());
    ctx.renderer
        .render_list_or_empty(&rows, "No organizations found")
}

/// Get organization details.
pub async fn get_organization(ctx: &JsmContext<'_>, org_id: i64) -> Result<()> {
    let path = format!("/rest/servicedeskapi/organization/{org_id}");
    tracing::debug!("Fetching organization {}", org_id);

    let org: Organization = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch organization {}", org_id))?;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
    }

    let view = View {
        id: &org.id,
        name: &org.name,
    };

    tracing::info!("Retrieved organization {}", org.name);
    ctx.renderer.render(&view)
}

/// Create an organization.
pub async fn create_organization(ctx: &JsmContext<'_>, name: String) -> Result<()> {
    #[derive(Serialize)]
    struct CreateOrgBody {
        name: String,
    }

    let body = CreateOrgBody { name: name.clone() };

    tracing::debug!("Creating organization: {}", name);
    let org: Organization = ctx
        .client
        .post("/rest/servicedeskapi/organization", &body)
        .await
        .context("Failed to create organization")?;

    tracing::info!("Created organization {} (id: {})", org.name, org.id);
    println!("Created organization: {} (id: {})", org.name, org.id);
    Ok(())
}

/// Delete an organization.
pub async fn delete_organization(ctx: &JsmContext<'_>, org_id: i64) -> Result<()> {
    let path = format!("/rest/servicedeskapi/organization/{org_id}");

    tracing::debug!("Deleting organization {}", org_id);
    ctx.client
        .delete::<()>(&path)
        .await
        .with_context(|| format!("Failed to delete organization {}", org_id))?;

    tracing::info!("Deleted organization {}", org_id);
    println!("Successfully deleted organization {}", org_id);
    Ok(())
}

/// List users in an organization.
pub async fn list_organization_users(
    ctx: &JsmContext<'_>,
    org_id: i64,
    limit: usize,
) -> Result<()> {
    #[derive(Deserialize)]
    struct UserList {
        values: Vec<User>,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("limit", &limit.min(50).to_string())
        .finish();
    let path = format!("/rest/servicedeskapi/organization/{org_id}/user?{query}");

    tracing::debug!("Listing users in organization {}", org_id);
    let response: UserList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list users in organization {}", org_id))?;

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
        tracing::info!("No users in organization {}", org_id);
    }

    tracing::info!("Found {} users in organization {}", rows.len(), org_id);
    ctx.renderer.render_list_or_empty(&rows, "No users found")
}

/// Add users to an organization.
pub async fn add_organization_user(
    ctx: &JsmContext<'_>,
    org_id: i64,
    account_ids: Vec<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct AddUserBody {
        #[serde(rename = "accountIds")]
        account_ids: Vec<String>,
    }

    let path = format!("/rest/servicedeskapi/organization/{org_id}/user");
    let body = AddUserBody {
        account_ids: account_ids.clone(),
    };

    tracing::debug!(
        "Adding {} users to organization {}",
        account_ids.len(),
        org_id
    );
    ctx.client
        .post::<(), _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add users to organization {}", org_id))?;

    tracing::info!(
        "Added {} users to organization {}",
        account_ids.len(),
        org_id
    );
    println!(
        "Successfully added {} user(s) to organization {}",
        account_ids.len(),
        org_id
    );
    Ok(())
}

/// Remove users from an organization.
pub async fn remove_organization_user(
    ctx: &JsmContext<'_>,
    org_id: i64,
    account_ids: Vec<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct RemoveUserBody {
        #[serde(rename = "accountIds")]
        account_ids: Vec<String>,
    }

    let path = format!("/rest/servicedeskapi/organization/{org_id}/user");
    let body = RemoveUserBody {
        account_ids: account_ids.clone(),
    };

    tracing::debug!(
        "Removing {} users from organization {}",
        account_ids.len(),
        org_id
    );
    ctx.client
        .delete_with_body::<(), _>(&path, &body)
        .await
        .with_context(|| format!("Failed to remove users from organization {}", org_id))?;

    tracing::info!(
        "Removed {} users from organization {}",
        account_ids.len(),
        org_id
    );
    println!(
        "Successfully removed {} user(s) from organization {}",
        account_ids.len(),
        org_id
    );
    Ok(())
}
