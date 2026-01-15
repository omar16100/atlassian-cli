use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{BambooContext, ListResponse, Project, ProjectsWrapper};

/// List all projects.
pub async fn list_projects(ctx: &BambooContext<'_>, limit: usize) -> Result<()> {
    let path = format!("/rest/api/latest/project?max-result={}", limit.min(100));
    tracing::debug!("Listing projects with limit {}", limit);

    #[derive(serde::Deserialize)]
    struct Response {
        projects: ListResponse<ProjectsWrapper>,
    }

    let response: Response = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list projects")?;

    let projects = response.projects.items.project.unwrap_or_default();

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        name: &'a str,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = projects
        .iter()
        .map(|project| Row {
            key: &project.key,
            name: &project.name,
            description: project.description.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No projects found");
    } else {
        tracing::info!("Found {} projects", rows.len());
    }
    ctx.renderer.render(&rows)
}

/// Get project details.
pub async fn get_project(ctx: &BambooContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/api/latest/project/{}", key);
    tracing::debug!("Fetching project {}", key);

    let project: Project = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch project {}", key))?;

    #[derive(Serialize)]
    struct View<'a> {
        key: &'a str,
        name: &'a str,
        description: &'a str,
    }

    let view = View {
        key: &project.key,
        name: &project.name,
        description: project.description.as_deref().unwrap_or(""),
    };

    tracing::info!("Retrieved project {}", key);
    ctx.renderer.render(&view)
}
