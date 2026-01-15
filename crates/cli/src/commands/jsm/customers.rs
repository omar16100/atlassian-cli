use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{JsmContext, User};

/// Create a customer.
pub async fn create_customer(
    ctx: &JsmContext<'_>,
    email: String,
    display_name: String,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateCustomerBody {
        email: String,
        #[serde(rename = "displayName")]
        display_name: String,
    }

    let body = CreateCustomerBody {
        email: email.clone(),
        display_name: display_name.clone(),
    };

    tracing::debug!("Creating customer: {} ({})", display_name, email);
    let customer: User = ctx
        .client
        .post("/rest/servicedeskapi/customer", &body)
        .await
        .context("Failed to create customer")?;

    tracing::info!(
        "Created customer {} (account_id: {})",
        display_name,
        customer.account_id
    );
    println!(
        "Created customer: {} (account_id: {})",
        display_name, customer.account_id
    );
    Ok(())
}

/// Revoke portal-only access for a user.
pub async fn revoke_portal_access(ctx: &JsmContext<'_>, account_id: &str) -> Result<()> {
    let path = format!("/rest/servicedeskapi/customer/user/{account_id}/revoke-portal-only-access");

    tracing::debug!("Revoking portal access for user {}", account_id);
    ctx.client
        .put::<(), _>(&path, &serde_json::json!({}))
        .await
        .with_context(|| format!("Failed to revoke portal access for user {}", account_id))?;

    tracing::info!("Revoked portal access for user {}", account_id);
    println!("Successfully revoked portal access for user {}", account_id);
    Ok(())
}
