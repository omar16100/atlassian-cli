use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;

use super::utils::JiraContext;

// Role Management Functions

pub async fn list_roles(ctx: &JiraContext<'_>, project: &str) -> Result<()> {
    let roles: Value = ctx
        .client
        .get(&format!("/rest/api/3/project/{project}/role"))
        .await
        .with_context(|| format!("Failed to list roles for project {project}"))?;

    println!("Roles for project {}:", project);
    println!("{}", serde_json::to_string_pretty(&roles)?);
    Ok(())
}

pub async fn get_role(ctx: &JiraContext<'_>, project: &str, role_id: &str) -> Result<()> {
    let role: Value = ctx
        .client
        .get(&format!("/rest/api/3/project/{project}/role/{role_id}"))
        .await
        .with_context(|| format!("Failed to get role {role_id} for project {project}"))?;

    println!("{}", serde_json::to_string_pretty(&role)?);
    Ok(())
}

pub async fn list_role_actors(ctx: &JiraContext<'_>, project: &str, role_id: &str) -> Result<()> {
    let role: Value = ctx
        .client
        .get(&format!("/rest/api/3/project/{project}/role/{role_id}"))
        .await
        .with_context(|| format!("Failed to get role {role_id} for project {project}"))?;

    if let Some(actors) = role.get("actors") {
        println!("Actors for role {}:", role_id);
        println!("{}", serde_json::to_string_pretty(actors)?);
    } else {
        println!("No actors found for role {}", role_id);
    }
    Ok(())
}

pub async fn add_role_actor(
    ctx: &JiraContext<'_>,
    project: &str,
    role_id: &str,
    user: &str,
) -> Result<()> {
    use serde_json::json;

    let payload = json!({ "user": [user] });

    let _: Value = ctx
        .client
        .post(
            &format!("/rest/api/3/project/{project}/role/{role_id}"),
            &payload,
        )
        .await
        .with_context(|| format!("Failed to add actor to role {role_id}"))?;

    tracing::info!(%project, %role_id, %user, "Actor added to role successfully");
    println!(
        "✅ Added {} to role {} in project {}",
        user, role_id, project
    );
    Ok(())
}

pub async fn remove_role_actor(
    ctx: &JiraContext<'_>,
    project: &str,
    role_id: &str,
    user: &str,
) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!(
            "/rest/api/3/project/{project}/role/{role_id}?user={user}"
        ))
        .await
        .with_context(|| format!("Failed to remove actor from role {role_id}"))?;

    tracing::info!(%project, %role_id, %user, "Actor removed from role successfully");
    println!(
        "✅ Removed {} from role {} in project {}",
        user, role_id, project
    );
    Ok(())
}

// Field Management Functions

pub async fn list_fields(ctx: &JiraContext<'_>) -> Result<()> {
    #[derive(Deserialize)]
    struct Field {
        id: String,
        name: String,
        custom: bool,
        #[serde(default)]
        description: Option<String>,
    }

    let fields: Vec<Field> = ctx
        .client
        .get("/rest/api/3/field")
        .await
        .context("Failed to list fields")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        custom: bool,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = fields
        .iter()
        .map(|f| Row {
            id: f.id.as_str(),
            name: f.name.as_str(),
            custom: f.custom,
            description: f.description.as_deref().unwrap_or(""),
        })
        .collect();

    ctx.renderer.render(&rows)
}

pub async fn get_field(ctx: &JiraContext<'_>, id: &str) -> Result<()> {
    let field: Value = ctx
        .client
        .get(&format!("/rest/api/3/field/{id}"))
        .await
        .with_context(|| format!("Failed to get field {id}"))?;

    println!("{}", serde_json::to_string_pretty(&field)?);
    Ok(())
}

pub async fn create_field(
    ctx: &JiraContext<'_>,
    name: &str,
    description: Option<&str>,
    field_type: &str,
) -> Result<()> {
    use serde_json::json;

    let payload = json!({
        "name": name,
        "description": description.unwrap_or(""),
        "type": field_type,
        "searcherKey": "com.atlassian.jira.plugin.system.customfieldtypes:textsearcher",
    });

    #[derive(Deserialize)]
    struct CreateResponse {
        id: String,
        name: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/rest/api/3/field", &payload)
        .await
        .context("Failed to create custom field")?;

    tracing::info!(id = %response.id, name = %response.name, "Custom field created successfully");
    println!(
        "✅ Created custom field: {} (ID: {})",
        response.name, response.id
    );
    Ok(())
}

pub async fn delete_field(ctx: &JiraContext<'_>, id: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/field/{id}"))
        .await
        .with_context(|| format!("Failed to delete field {id}"))?;

    tracing::info!(%id, "Custom field deleted successfully");
    println!("✅ Deleted custom field: {}", id);
    Ok(())
}

// Workflow Management Functions

pub async fn list_workflows(ctx: &JiraContext<'_>) -> Result<()> {
    #[derive(Deserialize)]
    struct WorkflowsResponse {
        values: Vec<WorkflowInfo>,
    }

    #[derive(Deserialize)]
    struct WorkflowInfo {
        id: WorkflowId,
        description: String,
    }

    #[derive(Deserialize)]
    struct WorkflowId {
        name: String,
        #[serde(rename = "entityId")]
        entity_id: String,
    }

    let response: WorkflowsResponse = ctx
        .client
        .get("/rest/api/3/workflow/search")
        .await
        .context("Failed to list workflows")?;

    #[derive(Serialize)]
    struct Row<'a> {
        name: &'a str,
        entity_id: &'a str,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|w| Row {
            name: w.id.name.as_str(),
            entity_id: w.id.entity_id.as_str(),
            description: w.description.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

pub async fn get_workflow(ctx: &JiraContext<'_>, name: &str) -> Result<()> {
    // Note: This is simplified - real implementation would search by name first
    let workflows: Value = ctx
        .client
        .get(&format!("/rest/api/3/workflow/search?workflowName={name}"))
        .await
        .with_context(|| format!("Failed to get workflow {name}"))?;

    println!("{}", serde_json::to_string_pretty(&workflows)?);
    Ok(())
}

pub async fn export_workflow(
    ctx: &JiraContext<'_>,
    name: &str,
    output: Option<&str>,
) -> Result<()> {
    let workflow: Value = ctx
        .client
        .get(&format!("/rest/api/3/workflow/search?workflowName={name}"))
        .await
        .with_context(|| format!("Failed to export workflow {name}"))?;

    let json_str = serde_json::to_string_pretty(&workflow)?;

    if let Some(path) = output {
        fs::write(path, json_str)?;
        println!("✅ Exported workflow {} to {}", name, path);
    } else {
        println!("{}", json_str);
    }

    Ok(())
}
