use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::BitbucketContext;

#[derive(Deserialize)]
struct PipelineList {
    values: Vec<Pipeline>,
}

#[derive(Deserialize)]
struct Pipeline {
    uuid: String,
    #[serde(default)]
    build_number: Option<i64>,
    #[serde(default)]
    state: Option<State>,
    #[serde(default)]
    created_on: Option<String>,
    #[serde(default)]
    completed_on: Option<String>,
    #[serde(default)]
    target: Option<Target>,
}

#[derive(Deserialize)]
struct State {
    name: String,
}

#[derive(Deserialize)]
struct Target {
    #[serde(default)]
    ref_name: Option<String>,
    #[serde(rename = "type", default)]
    target_type: Option<String>,
}

pub async fn list_pipelines(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    limit: usize,
) -> Result<()> {
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("pagelen", &limit.min(100).to_string())
        .finish();
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines?{query}");

    let response: PipelineList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list pipelines for {workspace}/{repo_slug}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        build_number: String,
        state: &'a str,
        ref_name: &'a str,
        target_type: &'a str,
        created: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|pipeline| Row {
            build_number: pipeline
                .build_number
                .map(|n| n.to_string())
                .unwrap_or_default(),
            state: pipeline
                .state
                .as_ref()
                .map(|s| s.name.as_str())
                .unwrap_or(""),
            ref_name: pipeline
                .target
                .as_ref()
                .and_then(|t| t.ref_name.as_deref())
                .unwrap_or(""),
            target_type: pipeline
                .target
                .as_ref()
                .and_then(|t| t.target_type.as_deref())
                .unwrap_or(""),
            created: pipeline.created_on.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No pipelines found for repository");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn get_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}");
    let pipeline: Pipeline = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to fetch pipeline {pipeline_uuid} for {workspace}/{repo_slug}")
    })?;

    #[derive(Serialize)]
    struct View<'a> {
        uuid: &'a str,
        build_number: String,
        state: &'a str,
        ref_name: &'a str,
        created: &'a str,
        completed: &'a str,
    }

    let view = View {
        uuid: pipeline.uuid.as_str(),
        build_number: pipeline
            .build_number
            .map(|n| n.to_string())
            .unwrap_or_default(),
        state: pipeline
            .state
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or(""),
        ref_name: pipeline
            .target
            .as_ref()
            .and_then(|t| t.ref_name.as_deref())
            .unwrap_or(""),
        created: pipeline.created_on.as_deref().unwrap_or(""),
        completed: pipeline.completed_on.as_deref().unwrap_or(""),
    };

    ctx.renderer.render(&view)
}

pub async fn trigger_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    ref_name: &str,
    ref_type: &str,
) -> Result<()> {
    let payload = serde_json::json!({
        "target": {
            "ref_name": ref_name,
            "ref_type": ref_type,
            "type": "pipeline_ref_target"
        }
    });

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/");
    let pipeline: Pipeline = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to trigger pipeline for {ref_name} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        build_number = pipeline.build_number,
        ref_name,
        workspace,
        repo_slug,
        "Pipeline triggered successfully"
    );

    #[derive(Serialize)]
    struct Triggered<'a> {
        uuid: &'a str,
        build_number: String,
        state: &'a str,
        ref_name: &'a str,
    }

    let triggered = Triggered {
        uuid: pipeline.uuid.as_str(),
        build_number: pipeline
            .build_number
            .map(|n| n.to_string())
            .unwrap_or_default(),
        state: pipeline
            .state
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or(""),
        ref_name: pipeline
            .target
            .as_ref()
            .and_then(|t| t.ref_name.as_deref())
            .unwrap_or(""),
    };

    ctx.renderer.render(&triggered)
}

pub async fn stop_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
) -> Result<()> {
    let path =
        format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/stopPipeline");
    let _: serde_json::Value = ctx
        .client
        .post(&path, &serde_json::json!({}))
        .await
        .with_context(|| {
            format!("Failed to stop pipeline {pipeline_uuid} on {workspace}/{repo_slug}")
        })?;

    tracing::info!(
        pipeline_uuid,
        workspace,
        repo_slug,
        "Pipeline stopped successfully"
    );

    println!("✓ Pipeline {pipeline_uuid} stopped on {workspace}/{repo_slug}");
    Ok(())
}

pub async fn get_pipeline_logs(
    _ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
    step_uuid: &str,
) -> Result<()> {
    tracing::info!(
        pipeline_uuid,
        step_uuid,
        workspace,
        repo_slug,
        "Fetching pipeline logs"
    );

    println!("Pipeline logs for step {step_uuid}:");
    println!(
        "View at: https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{}/steps/{}",
        pipeline_uuid.trim_matches('{').trim_matches('}'),
        step_uuid.trim_matches('{').trim_matches('}')
    );
    println!("\nNote: Use the web interface to view full logs with syntax highlighting");

    Ok(())
}
