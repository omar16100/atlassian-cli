use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::utils::JiraContext;

// Project Operations

pub async fn list_projects(ctx: &JiraContext<'_>) -> Result<()> {
    #[derive(Deserialize)]
    struct ProjectsResponse {
        #[serde(default)]
        values: Vec<Project>,
    }

    #[derive(Deserialize)]
    struct Project {
        key: String,
        name: String,
        #[serde(default)]
        project_type_key: Option<String>,
        #[serde(default)]
        lead: Option<UserField>,
    }

    #[derive(Deserialize)]
    struct UserField {
        #[serde(rename = "displayName")]
        display_name: String,
    }

    let response: ProjectsResponse = ctx
        .client
        .get("/rest/api/3/project/search")
        .await
        .context("Failed to list projects")?;

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        name: &'a str,
        lead: &'a str,
        project_type: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|project| Row {
            key: project.key.as_str(),
            name: project.name.as_str(),
            lead: project
                .lead
                .as_ref()
                .map(|lead| lead.display_name.as_str())
                .unwrap_or(""),
            project_type: project.project_type_key.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No projects returned for this account.");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn get_project(ctx: &JiraContext<'_>, key: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct ProjectDetails {
        #[allow(dead_code)]
        id: String,
        key: String,
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        lead: Option<UserField>,
        #[serde(rename = "projectTypeKey", default)]
        project_type_key: Option<String>,
        #[serde(default)]
        #[allow(dead_code)]
        url: Option<String>,
    }

    #[derive(Deserialize)]
    struct UserField {
        #[serde(rename = "displayName")]
        display_name: String,
    }

    let project: ProjectDetails = ctx
        .client
        .get(&format!("/rest/api/3/project/{key}"))
        .await
        .with_context(|| format!("Failed to get project {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        name: &'a str,
        lead: &'a str,
        project_type: &'a str,
        description: &'a str,
    }

    let row = Row {
        key: project.key.as_str(),
        name: project.name.as_str(),
        lead: project
            .lead
            .as_ref()
            .map(|l| l.display_name.as_str())
            .unwrap_or(""),
        project_type: project.project_type_key.as_deref().unwrap_or(""),
        description: project.description.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&[row])
}

pub async fn create_project(
    ctx: &JiraContext<'_>,
    key: &str,
    name: &str,
    project_type: &str,
    lead: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    use serde_json::json;

    let mut payload = json!({
        "key": key,
        "name": name,
        "projectTypeKey": project_type,
    });

    if let Some(lead_id) = lead {
        payload["leadAccountId"] = json!(lead_id);
    }

    if let Some(desc) = description {
        payload["description"] = json!(desc);
    }

    #[derive(Deserialize)]
    struct CreateResponse {
        key: String,
        id: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/rest/api/3/project", &payload)
        .await
        .context("Failed to create project")?;

    tracing::info!(key = %response.key, id = %response.id, "Project created successfully");
    println!("✅ Created project: {}", response.key);
    Ok(())
}

pub async fn update_project(
    ctx: &JiraContext<'_>,
    key: &str,
    name: Option<&str>,
    description: Option<&str>,
    lead: Option<&str>,
) -> Result<()> {
    use serde_json::json;

    let mut payload = json!({});

    if let Some(n) = name {
        payload["name"] = json!(n);
    }

    if let Some(desc) = description {
        payload["description"] = json!(desc);
    }

    if let Some(lead_id) = lead {
        payload["leadAccountId"] = json!(lead_id);
    }

    let _: Value = ctx
        .client
        .put(&format!("/rest/api/3/project/{key}"), &payload)
        .await
        .with_context(|| format!("Failed to update project {key}"))?;

    tracing::info!(%key, "Project updated successfully");
    println!("✅ Updated project: {}", key);
    Ok(())
}

pub async fn delete_project(ctx: &JiraContext<'_>, key: &str, force: bool) -> Result<()> {
    if !force {
        println!("⚠️  About to delete project: {}", key);
        println!("Use --force to confirm deletion");
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/project/{key}"))
        .await
        .with_context(|| format!("Failed to delete project {key}"))?;

    tracing::info!(%key, "Project deleted successfully");
    println!("✅ Deleted project: {}", key);
    Ok(())
}

// Component Management Functions

pub async fn list_components(ctx: &JiraContext<'_>, project: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Component {
        id: String,
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        lead: Option<UserField>,
    }

    #[derive(Deserialize)]
    struct UserField {
        #[serde(rename = "displayName")]
        display_name: String,
    }

    let components: Vec<Component> = ctx
        .client
        .get(&format!("/rest/api/3/project/{project}/components"))
        .await
        .with_context(|| format!("Failed to list components for project {project}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        lead: &'a str,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = components
        .iter()
        .map(|c| Row {
            id: c.id.as_str(),
            name: c.name.as_str(),
            lead: c
                .lead
                .as_ref()
                .map(|l| l.display_name.as_str())
                .unwrap_or(""),
            description: c.description.as_deref().unwrap_or(""),
        })
        .collect();

    ctx.renderer.render(&rows)
}

pub async fn get_component(ctx: &JiraContext<'_>, id: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Component {
        id: String,
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        lead: Option<UserField>,
        project: String,
    }

    #[derive(Deserialize)]
    struct UserField {
        #[serde(rename = "displayName")]
        display_name: String,
    }

    let component: Component = ctx
        .client
        .get(&format!("/rest/api/3/component/{id}"))
        .await
        .with_context(|| format!("Failed to get component {id}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        project: &'a str,
        lead: &'a str,
        description: &'a str,
    }

    let row = Row {
        id: component.id.as_str(),
        name: component.name.as_str(),
        project: component.project.as_str(),
        lead: component
            .lead
            .as_ref()
            .map(|l| l.display_name.as_str())
            .unwrap_or(""),
        description: component.description.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&[row])
}

pub async fn create_component(
    ctx: &JiraContext<'_>,
    project: &str,
    name: &str,
    description: Option<&str>,
    lead: Option<&str>,
) -> Result<()> {
    use serde_json::json;

    let mut payload = json!({
        "name": name,
        "project": project,
    });

    if let Some(desc) = description {
        payload["description"] = json!(desc);
    }

    if let Some(lead_id) = lead {
        payload["leadAccountId"] = json!(lead_id);
    }

    #[derive(Deserialize)]
    struct CreateResponse {
        id: String,
        name: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/rest/api/3/component", &payload)
        .await
        .context("Failed to create component")?;

    tracing::info!(id = %response.id, name = %response.name, "Component created successfully");
    println!(
        "✅ Created component: {} (ID: {})",
        response.name, response.id
    );
    Ok(())
}

pub async fn update_component(
    ctx: &JiraContext<'_>,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    use serde_json::json;

    let mut payload = json!({});

    if let Some(n) = name {
        payload["name"] = json!(n);
    }

    if let Some(desc) = description {
        payload["description"] = json!(desc);
    }

    let _: Value = ctx
        .client
        .put(&format!("/rest/api/3/component/{id}"), &payload)
        .await
        .with_context(|| format!("Failed to update component {id}"))?;

    tracing::info!(%id, "Component updated successfully");
    println!("✅ Updated component: {}", id);
    Ok(())
}

pub async fn delete_component(ctx: &JiraContext<'_>, id: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/component/{id}"))
        .await
        .with_context(|| format!("Failed to delete component {id}"))?;

    tracing::info!(%id, "Component deleted successfully");
    println!("✅ Deleted component: {}", id);
    Ok(())
}

// Version Management Functions

pub async fn list_versions(ctx: &JiraContext<'_>, project: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Version {
        id: String,
        name: String,
        #[serde(default)]
        #[allow(dead_code)]
        description: Option<String>,
        #[serde(default)]
        released: bool,
        #[serde(default)]
        archived: bool,
        #[serde(rename = "releaseDate", default)]
        release_date: Option<String>,
    }

    let versions: Vec<Version> = ctx
        .client
        .get(&format!("/rest/api/3/project/{project}/versions"))
        .await
        .with_context(|| format!("Failed to list versions for project {project}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        released: bool,
        archived: bool,
        release_date: &'a str,
    }

    let rows: Vec<Row<'_>> = versions
        .iter()
        .map(|v| Row {
            id: v.id.as_str(),
            name: v.name.as_str(),
            released: v.released,
            archived: v.archived,
            release_date: v.release_date.as_deref().unwrap_or(""),
        })
        .collect();

    ctx.renderer.render(&rows)
}

pub async fn get_version(ctx: &JiraContext<'_>, id: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Version {
        id: String,
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        released: bool,
        #[serde(default)]
        archived: bool,
        #[serde(rename = "releaseDate", default)]
        release_date: Option<String>,
        #[serde(rename = "startDate", default)]
        start_date: Option<String>,
    }

    let version: Version = ctx
        .client
        .get(&format!("/rest/api/3/version/{id}"))
        .await
        .with_context(|| format!("Failed to get version {id}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        released: bool,
        archived: bool,
        start_date: &'a str,
        release_date: &'a str,
        description: &'a str,
    }

    let row = Row {
        id: version.id.as_str(),
        name: version.name.as_str(),
        released: version.released,
        archived: version.archived,
        start_date: version.start_date.as_deref().unwrap_or(""),
        release_date: version.release_date.as_deref().unwrap_or(""),
        description: version.description.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&[row])
}

#[allow(clippy::too_many_arguments)]
pub async fn create_version(
    ctx: &JiraContext<'_>,
    project: &str,
    name: &str,
    description: Option<&str>,
    start_date: Option<&str>,
    release_date: Option<&str>,
    released: bool,
    archived: bool,
) -> Result<()> {
    use serde_json::json;

    let mut payload = json!({
        "name": name,
        "project": project,
        "released": released,
        "archived": archived,
    });

    if let Some(desc) = description {
        payload["description"] = json!(desc);
    }

    if let Some(date) = start_date {
        payload["startDate"] = json!(date);
    }

    if let Some(date) = release_date {
        payload["releaseDate"] = json!(date);
    }

    #[derive(Deserialize)]
    struct CreateResponse {
        id: String,
        name: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/rest/api/3/version", &payload)
        .await
        .context("Failed to create version")?;

    tracing::info!(id = %response.id, name = %response.name, "Version created successfully");
    println!(
        "✅ Created version: {} (ID: {})",
        response.name, response.id
    );
    Ok(())
}

pub async fn update_version(
    ctx: &JiraContext<'_>,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    released: Option<bool>,
    archived: Option<bool>,
) -> Result<()> {
    use serde_json::json;

    let mut payload = json!({});

    if let Some(n) = name {
        payload["name"] = json!(n);
    }

    if let Some(desc) = description {
        payload["description"] = json!(desc);
    }

    if let Some(r) = released {
        payload["released"] = json!(r);
    }

    if let Some(a) = archived {
        payload["archived"] = json!(a);
    }

    let _: Value = ctx
        .client
        .put(&format!("/rest/api/3/version/{id}"), &payload)
        .await
        .with_context(|| format!("Failed to update version {id}"))?;

    tracing::info!(%id, "Version updated successfully");
    println!("✅ Updated version: {}", id);
    Ok(())
}

pub async fn delete_version(ctx: &JiraContext<'_>, id: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/version/{id}"))
        .await
        .with_context(|| format!("Failed to delete version {id}"))?;

    tracing::info!(%id, "Version deleted successfully");
    println!("✅ Deleted version: {}", id);
    Ok(())
}

pub async fn merge_versions(ctx: &JiraContext<'_>, from: &str, to: &str) -> Result<()> {
    use serde_json::json;

    let _: Value = ctx
        .client
        .put(
            &format!("/rest/api/3/version/{from}/mergeto/{to}"),
            &json!({}),
        )
        .await
        .with_context(|| format!("Failed to merge version {from} to {to}"))?;

    tracing::info!(%from, %to, "Versions merged successfully");
    println!("✅ Merged version {} into {}", from, to);
    Ok(())
}
