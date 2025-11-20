use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::BitbucketContext;

#[derive(Deserialize)]
struct WebhookList {
    values: Vec<Webhook>,
}

#[derive(Deserialize)]
struct Webhook {
    uuid: String,
    url: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    events: Vec<String>,
}

#[derive(Deserialize)]
struct SshKeyList {
    values: Vec<SshKey>,
}

#[derive(Deserialize)]
struct SshKey {
    uuid: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    key: Option<String>,
}

pub async fn list_webhooks(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/hooks");

    let response: WebhookList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list webhooks for {workspace}/{repo_slug}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        uuid: &'a str,
        url: &'a str,
        active: bool,
        events_count: usize,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|webhook| Row {
            uuid: webhook.uuid.as_str(),
            url: webhook.url.as_str(),
            active: webhook.active,
            events_count: webhook.events.len(),
            description: webhook.description.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No webhooks found for repository");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn create_webhook(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    url: &str,
    description: Option<&str>,
    events: Vec<String>,
    active: bool,
) -> Result<()> {
    let mut payload = serde_json::json!({
        "url": url,
        "active": active,
        "events": events
    });

    if let Some(desc) = description {
        payload["description"] = serde_json::json!(desc);
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/hooks");
    let webhook: Webhook = ctx
        .client
        .post(&path, &payload)
        .await
        .with_context(|| format!("Failed to create webhook on {workspace}/{repo_slug}"))?;

    tracing::info!(
        webhook_uuid = webhook.uuid.as_str(),
        url,
        workspace,
        repo_slug,
        "Webhook created successfully"
    );

    #[derive(Serialize)]
    struct Created<'a> {
        uuid: &'a str,
        url: &'a str,
        active: bool,
        events_count: usize,
    }

    let created = Created {
        uuid: webhook.uuid.as_str(),
        url: webhook.url.as_str(),
        active: webhook.active,
        events_count: webhook.events.len(),
    };

    ctx.renderer.render(&created)
}

pub async fn delete_webhook(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    webhook_uuid: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/hooks/{webhook_uuid}");
    let _: serde_json::Value = ctx.client.delete(&path).await.with_context(|| {
        format!("Failed to delete webhook {webhook_uuid} from {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        webhook_uuid,
        workspace,
        repo_slug,
        "Webhook deleted successfully"
    );

    println!("✓ Webhook {webhook_uuid} deleted from {workspace}/{repo_slug}");
    Ok(())
}

pub async fn list_ssh_keys(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/deploy-keys");

    let response: SshKeyList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list SSH keys for {workspace}/{repo_slug}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        uuid: &'a str,
        label: &'a str,
        key_preview: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|key| Row {
            uuid: key.uuid.as_str(),
            label: key.label.as_deref().unwrap_or(""),
            key_preview: key
                .key
                .as_deref()
                .map(|k| {
                    let preview: String = k.chars().take(40).collect();
                    preview.leak() as &str
                })
                .unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No SSH keys found for repository");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn add_ssh_key(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    label: &str,
    key: &str,
) -> Result<()> {
    let payload = serde_json::json!({
        "label": label,
        "key": key
    });

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/deploy-keys");
    let ssh_key: SshKey = ctx
        .client
        .post(&path, &payload)
        .await
        .with_context(|| format!("Failed to add SSH key to {workspace}/{repo_slug}"))?;

    tracing::info!(
        key_uuid = ssh_key.uuid.as_str(),
        label,
        workspace,
        repo_slug,
        "SSH key added successfully"
    );

    println!("✓ SSH key '{label}' added to {workspace}/{repo_slug}");
    Ok(())
}

pub async fn delete_ssh_key(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    key_uuid: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/deploy-keys/{key_uuid}");
    let _: serde_json::Value = ctx.client.delete(&path).await.with_context(|| {
        format!("Failed to delete SSH key {key_uuid} from {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        key_uuid,
        workspace,
        repo_slug,
        "SSH key deleted successfully"
    );

    println!("✓ SSH key {key_uuid} deleted from {workspace}/{repo_slug}");
    Ok(())
}
