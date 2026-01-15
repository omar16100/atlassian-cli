use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{BambooContext, DeploymentProject, DeploymentResult, Environment};

/// List deployment projects.
pub async fn list_projects(ctx: &BambooContext<'_>, limit: usize) -> Result<()> {
    let path = format!(
        "/rest/api/latest/deploy/project/all?max-result={}",
        limit.min(100)
    );
    tracing::debug!("Listing deployment projects with limit {}", limit);

    let projects: Vec<DeploymentProject> = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list deployment projects")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        name: &'a str,
        plan_key: &'a str,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = projects
        .iter()
        .map(|project| Row {
            id: project.id,
            name: &project.name,
            plan_key: project
                .plan_key
                .as_ref()
                .map(|p| p.key.as_str())
                .unwrap_or(""),
            description: project.description.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No deployment projects found");
        println!("No deployment projects found");
        return Ok(());
    }

    tracing::info!("Found {} deployment projects", rows.len());
    ctx.renderer.render(&rows)
}

/// Get deployment project details.
pub async fn get_project(ctx: &BambooContext<'_>, id: i64) -> Result<()> {
    let path = format!("/rest/api/latest/deploy/project/{}", id);
    tracing::debug!("Fetching deployment project {}", id);

    let project: DeploymentProject = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch deployment project {}", id))?;

    #[derive(Serialize)]
    struct View<'a> {
        id: i64,
        name: &'a str,
        plan_key: &'a str,
        description: &'a str,
    }

    let view = View {
        id: project.id,
        name: &project.name,
        plan_key: project
            .plan_key
            .as_ref()
            .map(|p| p.key.as_str())
            .unwrap_or(""),
        description: project.description.as_deref().unwrap_or(""),
    };

    tracing::info!("Retrieved deployment project {}", id);
    ctx.renderer.render(&view)
}

/// List environments for a deployment project.
pub async fn list_environments(ctx: &BambooContext<'_>, project_id: i64) -> Result<()> {
    let path = format!("/rest/api/latest/deploy/project/{}", project_id);
    tracing::debug!("Listing environments for deployment project {}", project_id);

    #[derive(serde::Deserialize)]
    struct ProjectWithEnvs {
        environments: Vec<Environment>,
    }

    let project: ProjectWithEnvs = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list environments for project {}", project_id))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        name: &'a str,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = project
        .environments
        .iter()
        .map(|env| Row {
            id: env.id,
            name: &env.name,
            description: env.description.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No environments found for project {}", project_id);
        println!("No environments found");
        return Ok(());
    }

    tracing::info!(
        "Found {} environments for project {}",
        rows.len(),
        project_id
    );
    ctx.renderer.render(&rows)
}

/// Get environment details.
pub async fn get_environment(ctx: &BambooContext<'_>, env_id: i64) -> Result<()> {
    let path = format!("/rest/api/latest/deploy/environment/{}", env_id);
    tracing::debug!("Fetching environment {}", env_id);

    let env: Environment = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch environment {}", env_id))?;

    #[derive(Serialize)]
    struct View<'a> {
        id: i64,
        name: &'a str,
        description: &'a str,
        deployment_project_id: i64,
    }

    let view = View {
        id: env.id,
        name: &env.name,
        description: env.description.as_deref().unwrap_or(""),
        deployment_project_id: env.deployment_project_id.unwrap_or(0),
    };

    tracing::info!("Retrieved environment {}", env_id);
    ctx.renderer.render(&view)
}

/// List deployment results for an environment.
pub async fn list_results(ctx: &BambooContext<'_>, env_id: i64, limit: usize) -> Result<()> {
    let path = format!(
        "/rest/api/latest/deploy/environment/{}/results?max-result={}",
        env_id,
        limit.min(100)
    );
    tracing::debug!(
        "Listing deployment results for environment {} with limit {}",
        env_id,
        limit
    );

    #[derive(serde::Deserialize)]
    struct ResultsResponse {
        results: Vec<DeploymentResult>,
    }

    let response: ResultsResponse = ctx.client.get(&path).await.with_context(|| {
        format!(
            "Failed to list deployment results for environment {}",
            env_id
        )
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        version: &'a str,
        state: &'a str,
        lifecycle: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .results
        .iter()
        .map(|result| Row {
            id: result.id,
            version: result
                .deployment_version
                .as_ref()
                .map(|v| v.name.as_str())
                .unwrap_or(result.deployment_version_name.as_deref().unwrap_or("")),
            state: result.deployment_state.as_deref().unwrap_or(""),
            lifecycle: result.life_cycle_state.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No deployment results found for environment {}", env_id);
        println!("No deployment results found");
        return Ok(());
    }

    tracing::info!(
        "Found {} deployment results for environment {}",
        rows.len(),
        env_id
    );
    ctx.renderer.render(&rows)
}

/// Trigger a deployment.
pub async fn trigger_deployment(
    ctx: &BambooContext<'_>,
    env_id: i64,
    version_id: i64,
) -> Result<()> {
    #[derive(serde::Serialize)]
    struct TriggerBody {
        #[serde(rename = "environmentId")]
        environment_id: i64,
        #[serde(rename = "versionId")]
        version_id: i64,
    }

    let path = "/rest/api/latest/queue/deployment";
    let body = TriggerBody {
        environment_id: env_id,
        version_id,
    };

    tracing::debug!(
        "Triggering deployment to environment {} with version {}",
        env_id,
        version_id
    );

    #[derive(serde::Deserialize)]
    struct TriggerResponse {
        #[serde(rename = "deploymentResultId")]
        deployment_result_id: i64,
    }

    let response: TriggerResponse = ctx
        .client
        .post(path, &body)
        .await
        .with_context(|| format!("Failed to trigger deployment to environment {}", env_id))?;

    tracing::info!(
        "Triggered deployment (result id: {}) to environment {}",
        response.deployment_result_id,
        env_id
    );
    println!(
        "Deployment triggered (result id: {})",
        response.deployment_result_id
    );
    Ok(())
}

/// List versions for a deployment project.
pub async fn list_versions(ctx: &BambooContext<'_>, project_id: i64, limit: usize) -> Result<()> {
    let path = format!(
        "/rest/api/latest/deploy/project/{}/versions?max-result={}",
        project_id,
        limit.min(100)
    );
    tracing::debug!(
        "Listing versions for deployment project {} with limit {}",
        project_id,
        limit
    );

    #[derive(serde::Deserialize)]
    struct VersionsResponse {
        versions: Vec<Version>,
    }

    #[allow(dead_code)]
    #[derive(serde::Deserialize)]
    struct Version {
        id: i64,
        name: String,
        #[serde(rename = "creationDate")]
        creation_date: Option<i64>,
    }

    let response: VersionsResponse = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list versions for project {}", project_id))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        name: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .versions
        .iter()
        .map(|v| Row {
            id: v.id,
            name: &v.name,
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No versions found for project {}", project_id);
        println!("No versions found");
        return Ok(());
    }

    tracing::info!("Found {} versions for project {}", rows.len(), project_id);
    ctx.renderer.render(&rows)
}
