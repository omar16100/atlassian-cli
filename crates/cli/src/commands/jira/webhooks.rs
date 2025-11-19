use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::utils::JiraContext;

// List webhooks
pub async fn list_webhooks(ctx: &JiraContext<'_>) -> Result<()> {
    #[derive(Deserialize)]
    struct WebhooksResponse {
        values: Vec<Webhook>,
    }

    #[derive(Deserialize)]
    struct Webhook {
        id: i64,
        name: String,
        url: String,
        enabled: bool,
        events: Vec<String>,
    }

    let response: WebhooksResponse = ctx
        .client
        .get("/rest/webhooks/1.0/webhook")
        .await
        .context("Failed to list webhooks")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        name: &'a str,
        url: &'a str,
        enabled: bool,
        events: String,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|w| Row {
            id: w.id,
            name: w.name.as_str(),
            url: w.url.as_str(),
            enabled: w.enabled,
            events: w.events.join(", "),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Get webhook details
pub async fn get_webhook(ctx: &JiraContext<'_>, webhook_id: i64) -> Result<()> {
    let webhook: Value = ctx
        .client
        .get(&format!("/rest/webhooks/1.0/webhook/{webhook_id}"))
        .await
        .with_context(|| format!("Failed to get webhook {webhook_id}"))?;

    println!("{}", serde_json::to_string_pretty(&webhook)?);
    Ok(())
}

// Create webhook
pub async fn create_webhook(
    ctx: &JiraContext<'_>,
    name: &str,
    url: &str,
    events: Vec<String>,
    enabled: bool,
    jql_filter: Option<&str>,
) -> Result<()> {
    let mut payload = json!({
        "name": name,
        "url": url,
        "events": events,
        "enabled": enabled,
    });

    if let Some(filter) = jql_filter {
        payload["filters"] = json!({
            "issue-related-events-section": filter
        });
    }

    #[derive(Deserialize)]
    struct CreateResponse {
        id: i64,
        name: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/rest/webhooks/1.0/webhook", &payload)
        .await
        .context("Failed to create webhook")?;

    tracing::info!(id = %response.id, name = %response.name, "Webhook created successfully");
    println!(
        "✅ Created webhook: {} (ID: {})",
        response.name, response.id
    );
    Ok(())
}

// Update webhook
pub async fn update_webhook(
    ctx: &JiraContext<'_>,
    webhook_id: i64,
    name: Option<&str>,
    url: Option<&str>,
    events: Option<Vec<String>>,
    enabled: Option<bool>,
) -> Result<()> {
    // First get current webhook
    let current: Value = ctx
        .client
        .get(&format!("/rest/webhooks/1.0/webhook/{webhook_id}"))
        .await
        .with_context(|| format!("Failed to get webhook {webhook_id}"))?;

    let mut payload = current;

    if let Some(n) = name {
        payload["name"] = json!(n);
    }

    if let Some(u) = url {
        payload["url"] = json!(u);
    }

    if let Some(e) = events {
        payload["events"] = json!(e);
    }

    if let Some(en) = enabled {
        payload["enabled"] = json!(en);
    }

    let _: Value = ctx
        .client
        .put(
            &format!("/rest/webhooks/1.0/webhook/{webhook_id}"),
            &payload,
        )
        .await
        .with_context(|| format!("Failed to update webhook {webhook_id}"))?;

    tracing::info!(%webhook_id, "Webhook updated successfully");
    println!("✅ Updated webhook: {}", webhook_id);
    Ok(())
}

// Enable webhook
pub async fn enable_webhook(ctx: &JiraContext<'_>, webhook_id: i64) -> Result<()> {
    update_webhook(ctx, webhook_id, None, None, None, Some(true)).await
}

// Disable webhook
pub async fn disable_webhook(ctx: &JiraContext<'_>, webhook_id: i64) -> Result<()> {
    update_webhook(ctx, webhook_id, None, None, None, Some(false)).await
}

// Delete webhook
pub async fn delete_webhook(ctx: &JiraContext<'_>, webhook_id: i64, force: bool) -> Result<()> {
    if !force {
        println!(
            "⚠️  This will permanently delete webhook {}. Use --force to confirm.",
            webhook_id
        );
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!("/rest/webhooks/1.0/webhook/{webhook_id}"))
        .await
        .with_context(|| format!("Failed to delete webhook {webhook_id}"))?;

    tracing::info!(%webhook_id, "Webhook deleted successfully");
    println!("✅ Deleted webhook: {}", webhook_id);
    Ok(())
}

// Test webhook (send a test payload)
pub async fn test_webhook(ctx: &JiraContext<'_>, webhook_id: i64) -> Result<()> {
    let _: Value = ctx
        .client
        .post(
            &format!("/rest/webhooks/1.0/webhook/{webhook_id}/test"),
            &json!({}),
        )
        .await
        .with_context(|| format!("Failed to test webhook {webhook_id}"))?;

    tracing::info!(%webhook_id, "Webhook test sent successfully");
    println!("✅ Test payload sent to webhook: {}", webhook_id);
    Ok(())
}
