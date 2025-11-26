use anyhow::{Context, Result};
use atlassian_cli_api::ApiClient;
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::BitbucketContext;

#[derive(Deserialize)]
struct WorkspaceList {
    values: Vec<Workspace>,
}

#[derive(Deserialize)]
struct Workspace {
    slug: String,
    name: String,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(rename = "type", default)]
    workspace_type: Option<String>,
}

#[derive(Deserialize)]
struct ProjectList {
    values: Vec<Project>,
}

#[derive(Deserialize)]
struct Project {
    key: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    is_private: bool,
    #[serde(default)]
    uuid: Option<String>,
}

pub async fn list_workspaces(ctx: &BitbucketContext<'_>, limit: usize) -> Result<()> {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("pagelen", &limit.min(100).to_string())
        .finish();
    let path = format!("/2.0/workspaces?{query}");

    let response: WorkspaceList = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list workspaces")?;

    #[derive(Serialize)]
    struct Row<'a> {
        slug: &'a str,
        name: &'a str,
        workspace_type: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|ws| Row {
            slug: ws.slug.as_str(),
            name: ws.name.as_str(),
            workspace_type: ws.workspace_type.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No workspaces returned");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn get_workspace(ctx: &BitbucketContext<'_>, workspace: &str) -> Result<()> {
    let path = format!("/2.0/workspaces/{workspace}");
    let ws: Workspace = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch workspace {workspace}"))?;

    #[derive(Serialize)]
    struct View<'a> {
        slug: &'a str,
        name: &'a str,
        uuid: &'a str,
        workspace_type: &'a str,
    }

    let view = View {
        slug: ws.slug.as_str(),
        name: ws.name.as_str(),
        uuid: ws.uuid.as_deref().unwrap_or(""),
        workspace_type: ws.workspace_type.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&view)
}

pub async fn list_projects(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    limit: usize,
) -> Result<()> {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("pagelen", &limit.min(100).to_string())
        .finish();
    let path = format!("/2.0/workspaces/{workspace}/projects?{query}");

    let response: ProjectList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list projects in workspace {workspace}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        name: &'a str,
        description: &'a str,
        visibility: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|proj| Row {
            key: proj.key.as_str(),
            name: proj.name.as_str(),
            description: proj.description.as_deref().unwrap_or(""),
            visibility: if proj.is_private { "private" } else { "public" },
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, "No projects returned for workspace");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn get_project(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    project_key: &str,
) -> Result<()> {
    let path = format!("/2.0/workspaces/{workspace}/projects/{project_key}");
    let project: Project = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to fetch project {project_key} in workspace {workspace}")
    })?;

    #[derive(Serialize)]
    struct View<'a> {
        key: &'a str,
        name: &'a str,
        description: &'a str,
        visibility: &'a str,
        uuid: &'a str,
    }

    let view = View {
        key: project.key.as_str(),
        name: project.name.as_str(),
        description: project.description.as_deref().unwrap_or(""),
        visibility: if project.is_private {
            "private"
        } else {
            "public"
        },
        uuid: project.uuid.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&view)
}

pub async fn create_project(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    key: &str,
    name: &str,
    description: Option<&str>,
    is_private: bool,
) -> Result<()> {
    let mut payload = serde_json::json!({
        "key": key,
        "name": name,
        "is_private": is_private
    });

    if let Some(desc) = description {
        payload["description"] = serde_json::json!(desc);
    }

    let path = format!("/2.0/workspaces/{workspace}/projects");
    let project: Project = ctx
        .client
        .post(&path, &payload)
        .await
        .with_context(|| format!("Failed to create project in workspace {workspace}"))?;

    tracing::info!(
        project_key = project.key.as_str(),
        workspace,
        "Project created successfully"
    );

    #[derive(Serialize)]
    struct Created<'a> {
        key: &'a str,
        name: &'a str,
        visibility: &'a str,
    }

    let created = Created {
        key: project.key.as_str(),
        name: project.name.as_str(),
        visibility: if project.is_private {
            "private"
        } else {
            "public"
        },
    };

    ctx.renderer.render(&created)
}

pub async fn update_project(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    project_key: &str,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    let mut payload = serde_json::json!({});

    if let Some(n) = name {
        payload["name"] = serde_json::json!(n);
    }

    if let Some(d) = description {
        payload["description"] = serde_json::json!(d);
    }

    let path = format!("/2.0/workspaces/{workspace}/projects/{project_key}");
    let project: Project = ctx.client.put(&path, &payload).await.with_context(|| {
        format!("Failed to update project {project_key} in workspace {workspace}")
    })?;

    tracing::info!(
        project_key = project.key.as_str(),
        workspace,
        "Project updated successfully"
    );

    #[derive(Serialize)]
    struct Updated<'a> {
        key: &'a str,
        name: &'a str,
        description: &'a str,
    }

    let updated = Updated {
        key: project.key.as_str(),
        name: project.name.as_str(),
        description: project.description.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&updated)
}

pub async fn delete_project(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    project_key: &str,
    force: bool,
) -> Result<()> {
    if !force {
        use std::io::{self, Write};
        print!("Are you sure you want to delete project {project_key} from {workspace}? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            tracing::info!("Project deletion cancelled");
            return Ok(());
        }
    }

    let path = format!("/2.0/workspaces/{workspace}/projects/{project_key}");
    let _: serde_json::Value = ctx.client.delete(&path).await.with_context(|| {
        format!("Failed to delete project {project_key} from workspace {workspace}")
    })?;

    tracing::info!(project_key, workspace, "Project deleted successfully");

    println!("✓ Project {project_key} deleted from workspace {workspace}");
    Ok(())
}

#[derive(Deserialize)]
struct BitbucketUser {
    username: String,
    display_name: String,
    account_id: String,
    uuid: String,
}

pub async fn whoami(client: &ApiClient) -> Result<()> {
    let user: BitbucketUser = client
        .get("/2.0/user")
        .await
        .context("Failed to fetch current user from Bitbucket API")?;

    println!("Username: {}", user.username);
    println!("Display Name: {}", user.display_name);
    println!("Account ID: {}", user.account_id);
    println!("UUID: {}", user.uuid);

    Ok(())
}
