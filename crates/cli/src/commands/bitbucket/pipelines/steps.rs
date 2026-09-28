//! `pipeline steps`, and the step listing the other commands build on.

use anyhow::{Context, Result};
use atlassian_cli_api::pagination::{fetch_paged, BitbucketPage, PageLimits};
use atlassian_cli_output::OutputFormat;
use chrono::{DateTime, Utc};

use super::super::utils::BitbucketContext;
use super::list::resolve_pipeline_id;
use super::model::{PipelineStep, StepInfo};
use super::state::{format_duration_secs, get_step_status, normalize_trigger};

pub(super) async fn fetch_steps(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
    include_details: bool,
) -> Result<Vec<StepInfo>> {
    // Paginated. Bitbucket serves 10 steps per page by default, so build 568's
    // eleven steps came back as ten with nothing to say one was missing.
    let path = format!(
        "/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/?pagelen=100"
    );
    let (steps, page) =
        fetch_paged::<BitbucketPage<PipelineStep>>(&ctx.client, &path, PageLimits::new(None))
            .await
            .with_context(|| format!("Failed to fetch steps for pipeline {pipeline_uuid}"))?;

    if page.truncated {
        eprintln!(
            "warning: showing {} steps for pipeline {pipeline_uuid}; the list is incomplete.",
            steps.len()
        );
    }

    let clean_pipeline_uuid = pipeline_uuid.trim_matches('{').trim_matches('}');
    Ok(steps
        .iter()
        .map(|step| {
            let clean_step_uuid = step.uuid.trim_matches('{').trim_matches('}');
            StepInfo {
                uuid: step.uuid.clone(),
                name: step.name.clone().unwrap_or_else(|| step.uuid.clone()),
                status: get_step_status(step),
                started: if include_details {
                    step.started_on.clone()
                } else {
                    None
                },
                completed: if include_details {
                    step.completed_on.clone()
                } else {
                    None
                },
                duration: if include_details {
                    if let Some(secs) = step.duration_in_seconds {
                        Some(format_duration_secs(secs))
                    } else if step.started_on.is_some() && step.completed_on.is_none() {
                        // In-progress step: compute client-side elapsed time
                        step.started_on.as_ref().and_then(|started_str| {
                            DateTime::parse_from_rfc3339(started_str).ok().map(
                                |started| {
                                    let elapsed =
                                        Utc::now() - started.with_timezone(&Utc);
                                    let secs = elapsed.num_seconds().max(0) as u64;
                                    format!("~{}", format_duration_secs(secs))
                                },
                            )
                        })
                    } else {
                        None
                    }
                } else {
                    None
                },
                logs_url: if include_details {
                    Some(format!(
                        "https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{clean_pipeline_uuid}/steps/{clean_step_uuid}"
                    ))
                } else {
                    None
                },
                trigger: step
                    .trigger
                    .as_ref()
                    .and_then(|t| t.trigger_type.as_deref())
                    .map(normalize_trigger),
            }
        })
        .collect())
}

pub async fn list_steps(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_id: &str,
) -> Result<()> {
    // Resolve build number to UUID if needed
    let pipeline_uuid = resolve_pipeline_id(ctx, workspace, repo_slug, pipeline_id).await?;

    tracing::debug!(
        pipeline_uuid,
        workspace,
        repo_slug,
        "Listing pipeline steps"
    );

    let steps = fetch_steps(ctx, workspace, repo_slug, &pipeline_uuid, true).await?;

    if steps.is_empty() {
        tracing::info!(pipeline_uuid, "No steps found");
    }

    tracing::debug!(pipeline_uuid, count = steps.len(), "Listed pipeline steps");

    // `logs_url` is a full Bitbucket URL, long enough that including it pushes
    // the table past any terminal width and wraps every row into unreadability
    // -- the reported symptom was that JSON was the only usable format here.
    // The structured formats keep it: it is genuinely useful to a script, and
    // dropping it there would trade one broken format for another.
    match ctx.renderer.format() {
        OutputFormat::Table | OutputFormat::Markdown => {
            if steps.is_empty() {
                println!("No steps found");
                return Ok(());
            }
            let rows: Vec<serde_json::Value> = steps
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()?;
            let columns: Vec<String> = STEP_TABLE_COLUMNS.iter().map(|c| c.to_string()).collect();
            ctx.renderer.render_rows_ordered(&rows, &columns)
        }
        _ => ctx.renderer.render_list_or_empty(&steps, "No steps found"),
    }
}

/// Columns shown in the tabular views of `pipeline steps`.
///
/// Deliberately omits `logs_url`. `render_rows_ordered` applies one column list
/// to Table, CSV and Markdown alike, so this is only reached for the two
/// human-read formats; CSV and the structured formats go through the ordinary
/// path and keep every field.
/// `uuid` stays: it is the argument `bb pipeline logs <pipeline> <step-uuid>`
/// takes, so dropping it would have made the table unable to feed the command
/// it exists to support. Only `logs_url` is omitted, and only here.
pub(super) const STEP_TABLE_COLUMNS: [&str; 7] = [
    "uuid",
    "name",
    "status",
    "started",
    "completed",
    "duration",
    "trigger",
];

/// Check if a pipeline has failed steps
pub async fn pipeline_has_failed_steps(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pipeline_uuid: &str,
) -> Result<bool> {
    // Truncation here is worse than a short listing: a failure on the second
    // page would be invisible, and this decides the --wait exit status.
    let path = format!(
        "/2.0/repositories/{workspace}/{repo_slug}/pipelines/{pipeline_uuid}/steps/?pagelen=100"
    );
    let (steps, page) =
        fetch_paged::<BitbucketPage<PipelineStep>>(&ctx.client, &path, PageLimits::new(None))
            .await
            .with_context(|| format!("Failed to fetch steps for pipeline {pipeline_uuid}"))?;

    // The outcome, not `state.name`: a failed step is `COMPLETED` there, which
    // is why `rerun --pr --failed-only` used to skip every failed build.
    let found_failure = steps.iter().any(|step| {
        matches!(
            get_step_status(step).to_uppercase().as_str(),
            "FAILED" | "ERROR"
        )
    });

    // This decides an exit code. "I did not see a failure" is not the same as
    // "there was no failure" when the walk was cut short, and reporting the
    // former as success is how a broken build passes CI.
    if !found_failure && page.truncated {
        anyhow::bail!(
            "Could not determine whether pipeline {pipeline_uuid} failed: only {} steps could \
             be listed and no failure was seen among them.",
            steps.len()
        );
    }

    Ok(found_failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reported symptom: `logs_url` is a full Bitbucket URL, and including
    /// it pushed the steps table past any terminal width, leaving JSON as the
    /// only usable format.
    #[test]
    fn the_steps_table_omits_the_logs_url_column() {
        assert!(
            !STEP_TABLE_COLUMNS.contains(&"logs_url"),
            "logs_url must not be a table column"
        );
        for expected in ["uuid", "name", "status", "duration"] {
            assert!(
                STEP_TABLE_COLUMNS.contains(&expected),
                "{expected} should still be shown"
            );
        }
    }
}
