//! `pipeline trigger`, `stop` and `rerun`.

use anyhow::{Context, Result};
use serde::Serialize;

use super::super::utils::BitbucketContext;
use super::list::{fetch_pipeline, resolve_pipeline_id};
use super::model::{Pipeline, PipelineVariable};
use super::state::get_pipeline_status;
use crate::commands::common::{render_success, MutationResult};

pub(super) fn parse_variables(vars: Vec<String>, secured: bool) -> Result<Vec<PipelineVariable>> {
    let mut variables = Vec::new();

    for var_str in vars {
        let parts: Vec<&str> = var_str.splitn(2, '=').collect();

        if parts.len() != 2 {
            anyhow::bail!("Invalid variable format '{}'. Expected KEY=VALUE", var_str);
        }

        let key = parts[0].trim();
        let value = parts[1]; // Don't trim value - preserve whitespace

        if key.is_empty() {
            anyhow::bail!("Variable key cannot be empty in '{}'", var_str);
        }

        variables.push(PipelineVariable {
            key: key.to_string(),
            value: value.to_string(),
            secured,
        });
    }

    Ok(variables)
}

/// Build the POST body for triggering a pipeline. Pure and testable.
///
/// `custom_pipeline` injects a `target.selector` so a named custom pipeline from
/// `bitbucket-pipelines.yml` is run instead of the branch default. `variables`,
/// when non-empty, is serialized as the top-level `variables` array.
pub(super) fn build_trigger_payload(
    ref_name: &str,
    ref_type: &str,
    custom_pipeline: Option<&str>,
    variables: &[PipelineVariable],
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "target": {
            "ref_name": ref_name,
            "ref_type": ref_type,
            "type": "pipeline_ref_target"
        }
    });

    if let Some(name) = custom_pipeline {
        payload["target"]["selector"] = serde_json::json!({
            "type": "custom",
            "pattern": name
        });
    }

    if !variables.is_empty() {
        payload["variables"] = serde_json::json!(variables);
    }

    payload
}

#[allow(clippy::too_many_arguments)]
pub async fn trigger_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    ref_name: &str,
    ref_type: &str,
    variable_strings: Vec<String>,
    secured: bool,
    custom_pipeline: Option<String>,
) -> Result<()> {
    let variables = if variable_strings.is_empty() {
        Vec::new()
    } else {
        parse_variables(variable_strings.clone(), secured)?
    };

    let payload = build_trigger_payload(ref_name, ref_type, custom_pipeline.as_deref(), &variables);

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/");
    let pipeline: Pipeline = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to trigger pipeline for {ref_name} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        build_number = pipeline.build_number,
        ref_name,
        workspace,
        repo_slug,
        variables_count = variable_strings.len(),
        "Pipeline triggered successfully"
    );

    #[derive(Serialize)]
    struct Triggered {
        uuid: String,
        build_number: Option<i64>,
        state: String,
        ref_name: String,
    }

    let state = get_pipeline_status(&pipeline);
    let triggered = Triggered {
        uuid: pipeline.uuid,
        build_number: pipeline.build_number,
        state,
        ref_name: pipeline
            .target
            .as_ref()
            .and_then(|t| t.ref_name.clone())
            .unwrap_or_default(),
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

    render_success(
        ctx.renderer,
        &format!("✅ Pipeline {pipeline_uuid} stopped on {workspace}/{repo_slug}"),
        &MutationResult::with_id(
            format!("Pipeline stopped on {workspace}/{repo_slug}"),
            pipeline_uuid,
        ),
    )
}

pub async fn rerun_pipeline(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_id: &str,
    variable_strings: Vec<String>,
    secured: bool,
) -> Result<()> {
    // Resolve build number to UUID if needed
    let pipeline_uuid = resolve_pipeline_id(ctx, workspace, repo_slug, pipeline_id).await?;

    // Fetch original pipeline
    let original = fetch_pipeline(ctx, workspace, repo_slug, &pipeline_uuid).await?;

    tracing::info!(pipeline_id, workspace, repo_slug, "Re-running pipeline");

    // Extract commit hash or fallback to ref_name
    let mut payload = if let Some(commit_hash) = original
        .target
        .as_ref()
        .and_then(|t| t.commit.as_ref())
        .and_then(|c| c.hash.as_ref())
    {
        // Trigger with commit target
        serde_json::json!({
            "target": {
                "commit": {
                    "type": "commit",
                    "hash": commit_hash
                },
                "type": "pipeline_commit_target"
            }
        })
    } else {
        // Fallback to ref_name if no commit info
        let ref_name = original
            .target
            .as_ref()
            .and_then(|t| t.ref_name.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Pipeline has no commit or ref info"))?;

        let ref_type = original
            .target
            .as_ref()
            .and_then(|t| t.target_type.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("branch");

        serde_json::json!({
            "target": {
                "ref_name": ref_name,
                "ref_type": ref_type,
                "type": "pipeline_ref_target"
            }
        })
    };

    // Add variables if provided
    if !variable_strings.is_empty() {
        let variables = parse_variables(variable_strings.clone(), secured)?;
        payload["variables"] = serde_json::to_value(variables)?;
    }

    // Trigger new pipeline
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pipelines/");
    let new_pipeline: Pipeline = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to re-run pipeline {pipeline_id} on {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        original_id = pipeline_id,
        new_build_number = new_pipeline.build_number,
        variables_count = variable_strings.len(),
        "Pipeline re-run triggered"
    );

    // Return same format as trigger_pipeline
    #[derive(Serialize)]
    struct Triggered {
        uuid: String,
        build_number: Option<i64>,
        state: String,
        ref_name: String,
        rerun_from: String,
    }

    let state = get_pipeline_status(&new_pipeline);
    let triggered = Triggered {
        uuid: new_pipeline.uuid,
        build_number: new_pipeline.build_number,
        state,
        ref_name: new_pipeline
            .target
            .as_ref()
            .and_then(|t| t.ref_name.clone())
            .unwrap_or_default(),
        rerun_from: pipeline_id.to_string(),
    };

    ctx.renderer.render(&triggered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_variables_valid() {
        let vars = vec!["ENV=prod".to_string(), "DEBUG=true".to_string()];
        let result = parse_variables(vars, false).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "ENV");
        assert_eq!(result[0].value, "prod");
        assert!(!result[0].secured);
    }

    #[test]
    fn test_parse_variables_with_equals_in_value() {
        let vars = vec!["URL=http://example.com?foo=bar".to_string()];
        let result = parse_variables(vars, false).unwrap();
        assert_eq!(result[0].value, "http://example.com?foo=bar");
    }

    #[test]
    fn test_parse_variables_secured() {
        let vars = vec!["SECRET=value".to_string()];
        let result = parse_variables(vars, true).unwrap();
        assert!(result[0].secured);
    }

    #[test]
    fn test_parse_variables_invalid_format() {
        let vars = vec!["NOEQUALS".to_string()];
        let result = parse_variables(vars, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Expected KEY=VALUE"));
    }

    #[test]
    fn test_parse_variables_empty_key() {
        let vars = vec!["=value".to_string()];
        let result = parse_variables(vars, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("key cannot be empty"));
    }

    #[test]
    fn test_parse_variables_preserves_whitespace_in_value() {
        let vars = vec!["MSG= spaces  everywhere ".to_string()];
        let result = parse_variables(vars, false).unwrap();
        assert_eq!(result[0].value, " spaces  everywhere ");
    }

    #[test]
    fn test_build_trigger_payload_default_has_no_selector() {
        let payload = build_trigger_payload("main", "branch", None, &[]);
        assert_eq!(payload["target"]["ref_name"], "main");
        assert_eq!(payload["target"]["ref_type"], "branch");
        assert_eq!(payload["target"]["type"], "pipeline_ref_target");
        assert!(payload["target"].get("selector").is_none());
        assert!(payload.get("variables").is_none());
    }

    #[test]
    fn test_build_trigger_payload_injects_custom_selector() {
        let payload = build_trigger_payload("dev", "branch", Some("s3-access-test"), &[]);
        assert_eq!(payload["target"]["selector"]["type"], "custom");
        assert_eq!(payload["target"]["selector"]["pattern"], "s3-access-test");
    }

    #[test]
    fn test_build_trigger_payload_includes_variables() {
        let vars = vec![PipelineVariable {
            key: "ENV".to_string(),
            value: "prod".to_string(),
            secured: false,
        }];
        let payload = build_trigger_payload("main", "branch", None, &vars);
        assert_eq!(payload["variables"][0]["key"], "ENV");
        assert_eq!(payload["variables"][0]["value"], "prod");
        assert_eq!(payload["variables"][0]["secured"], false);
    }

    #[test]
    fn test_build_trigger_payload_selector_and_variables_together() {
        let vars = vec![PipelineVariable {
            key: "REGION".to_string(),
            value: "eu".to_string(),
            secured: true,
        }];
        let payload = build_trigger_payload("main", "branch", Some("deploy"), &vars);
        assert_eq!(payload["target"]["selector"]["pattern"], "deploy");
        assert_eq!(payload["variables"][0]["secured"], true);
    }
}
