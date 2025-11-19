use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;

use super::utils::JiraContext;

// List automation rules
pub async fn list_rules(ctx: &JiraContext<'_>) -> Result<()> {
    #[derive(Deserialize)]
    struct RulesResponse {
        values: Vec<AutomationRule>,
    }

    #[derive(Deserialize)]
    struct AutomationRule {
        id: i64,
        name: String,
        state: String,
        #[serde(rename = "authorAccountId")]
        author_account_id: String,
        created: i64,
    }

    let response: RulesResponse = ctx
        .client
        .get("/gateway/api/automation/internal-api/jira/cloud/rules")
        .await
        .context("Failed to list automation rules")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        name: &'a str,
        state: &'a str,
        author: &'a str,
        created: i64,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|r| Row {
            id: r.id,
            name: r.name.as_str(),
            state: r.state.as_str(),
            author: r.author_account_id.as_str(),
            created: r.created,
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Get automation rule details
pub async fn get_rule(ctx: &JiraContext<'_>, rule_id: i64) -> Result<()> {
    let rule: Value = ctx
        .client
        .get(&format!(
            "/gateway/api/automation/internal-api/jira/cloud/rules/{rule_id}"
        ))
        .await
        .with_context(|| format!("Failed to get automation rule {rule_id}"))?;

    println!("{}", serde_json::to_string_pretty(&rule)?);
    Ok(())
}

// Create automation rule
pub async fn create_rule(
    ctx: &JiraContext<'_>,
    name: &str,
    description: Option<&str>,
    definition_file: &std::path::PathBuf,
) -> Result<()> {
    let definition_content = fs::read_to_string(definition_file).with_context(|| {
        format!(
            "Failed to read definition file: {}",
            definition_file.display()
        )
    })?;

    let definition: Value = serde_json::from_str(&definition_content)
        .context("Failed to parse automation rule definition JSON")?;

    let mut payload = json!({
        "name": name,
        "state": "ENABLED",
        "ruleScope": definition.get("ruleScope").unwrap_or(&json!({"resources": []})),
        "trigger": definition.get("trigger").unwrap_or(&json!({})),
        "components": definition.get("components").unwrap_or(&json!([])),
    });

    if let Some(desc) = description {
        payload["description"] = json!(desc);
    }

    #[derive(Deserialize)]
    struct CreateResponse {
        id: i64,
        name: String,
    }

    let response: CreateResponse = ctx
        .client
        .post(
            "/gateway/api/automation/internal-api/jira/cloud/rules",
            &payload,
        )
        .await
        .context("Failed to create automation rule")?;

    tracing::info!(id = %response.id, name = %response.name, "Automation rule created successfully");
    println!(
        "✅ Created automation rule: {} (ID: {})",
        response.name, response.id
    );
    Ok(())
}

// Update automation rule
pub async fn update_rule(
    ctx: &JiraContext<'_>,
    rule_id: i64,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    // First get current rule
    let current_rule: Value = ctx
        .client
        .get(&format!(
            "/gateway/api/automation/internal-api/jira/cloud/rules/{rule_id}"
        ))
        .await
        .with_context(|| format!("Failed to get automation rule {rule_id}"))?;

    let mut payload = current_rule;

    if let Some(n) = name {
        payload["name"] = json!(n);
    }

    if let Some(d) = description {
        payload["description"] = json!(d);
    }

    let _: Value = ctx
        .client
        .put(
            &format!("/gateway/api/automation/internal-api/jira/cloud/rules/{rule_id}"),
            &payload,
        )
        .await
        .with_context(|| format!("Failed to update automation rule {rule_id}"))?;

    tracing::info!(%rule_id, "Automation rule updated successfully");
    println!("✅ Updated automation rule: {}", rule_id);
    Ok(())
}

// Enable automation rule
pub async fn enable_rule(ctx: &JiraContext<'_>, rule_id: i64) -> Result<()> {
    let _: Value = ctx
        .client
        .put(
            &format!("/gateway/api/automation/internal-api/jira/cloud/rules/{rule_id}/enable"),
            &json!({}),
        )
        .await
        .with_context(|| format!("Failed to enable automation rule {rule_id}"))?;

    tracing::info!(%rule_id, "Automation rule enabled successfully");
    println!("✅ Enabled automation rule: {}", rule_id);
    Ok(())
}

// Disable automation rule
pub async fn disable_rule(ctx: &JiraContext<'_>, rule_id: i64) -> Result<()> {
    let _: Value = ctx
        .client
        .put(
            &format!("/gateway/api/automation/internal-api/jira/cloud/rules/{rule_id}/disable"),
            &json!({}),
        )
        .await
        .with_context(|| format!("Failed to disable automation rule {rule_id}"))?;

    tracing::info!(%rule_id, "Automation rule disabled successfully");
    println!("✅ Disabled automation rule: {}", rule_id);
    Ok(())
}

// Delete automation rule
pub async fn delete_rule(ctx: &JiraContext<'_>, rule_id: i64, force: bool) -> Result<()> {
    if !force {
        println!(
            "⚠️  This will permanently delete automation rule {}. Use --force to confirm.",
            rule_id
        );
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!(
            "/gateway/api/automation/internal-api/jira/cloud/rules/{rule_id}"
        ))
        .await
        .with_context(|| format!("Failed to delete automation rule {rule_id}"))?;

    tracing::info!(%rule_id, "Automation rule deleted successfully");
    println!("✅ Deleted automation rule: {}", rule_id);
    Ok(())
}

// Export automation rule
pub async fn export_rule(
    ctx: &JiraContext<'_>,
    rule_id: i64,
    output: Option<&std::path::PathBuf>,
) -> Result<()> {
    let rule: Value = ctx
        .client
        .get(&format!(
            "/gateway/api/automation/internal-api/jira/cloud/rules/{rule_id}"
        ))
        .await
        .with_context(|| format!("Failed to export automation rule {rule_id}"))?;

    let json_str = serde_json::to_string_pretty(&rule)?;

    if let Some(path) = output {
        fs::write(path, json_str)?;
        println!(
            "✅ Exported automation rule {} to {}",
            rule_id,
            path.display()
        );
    } else {
        println!("{}", json_str);
    }

    Ok(())
}
