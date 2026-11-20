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
                        step.started_on
                            .as_ref()
                            .and_then(|started_str| running_step_duration(started_str, Utc::now()))
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

/// The duration text of a step that runs now: the time from `started_on` to
/// `now` in whole seconds, with a `~` before it. A start time in the future
/// gives `~0s`. A start time that is not valid RFC 3339 gives `None`.
fn running_step_duration(started_on: &str, now: DateTime<Utc>) -> Option<String> {
    let started = DateTime::parse_from_rfc3339(started_on).ok()?;
    let elapsed = now - started.with_timezone(&Utc);
    let secs = elapsed.num_seconds().max(0) as u64;
    Some(format!("~{}", format_duration_secs(secs)))
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

    // The `pinned_` tests pin the duration text of a running step for fixed
    // times. The expected values were recorded with `chrono` and did not
    // change when `chrono` was removed. Only the helper `running` knows the
    // time types.

    const STEP_NOW: &str = "2026-11-20T12:00:00Z";

    /// `running_step_duration` with the current time fixed at the RFC 3339
    /// time `now`.
    fn running(started_on: &str, now: &str) -> Option<String> {
        let now = DateTime::parse_from_rfc3339(now)
            .unwrap()
            .with_timezone(&Utc);
        running_step_duration(started_on, now)
    }

    #[test]
    fn pinned_the_duration_of_a_running_step() {
        for (started, expected) in [
            ("2026-11-20T12:00:00Z", "~0s"),
            ("2026-11-20T11:59:59Z", "~1s"),
            ("2026-11-20T11:59:01Z", "~59s"),
            ("2026-11-20T11:59:00Z", "~1m 0s"),
            ("2026-11-20T11:58:30Z", "~1m 30s"),
            ("2026-11-20T11:00:01Z", "~59m 59s"),
            ("2026-11-20T11:00:00Z", "~1h 0m 0s"),
            ("2026-11-20T08:54:55Z", "~3h 5m 5s"),
            ("2026-11-19T12:00:00Z", "~24h 0m 0s"),
            ("2026-11-10T10:30:15Z", "~241h 29m 45s"),
            // Bitbucket sends a fraction and `+00:00`.
            ("2026-11-20T11:58:30.123456+00:00", "~1m 29s"),
            // An offset is used.
            ("2026-11-20T19:58:30+08:00", "~1m 30s"),
            ("2026-11-20T06:58:30-05:00", "~1m 30s"),
            ("2026-11-20t11:58:30z", "~1m 30s"),
            ("2026-11-20 11:58:30Z", "~1m 30s"),
            ("0000-01-01T00:00:00Z", "~17767332h 0m 0s"),
        ] {
            assert_eq!(
                running(started, STEP_NOW).as_deref(),
                Some(expected),
                "started {started:?}"
            );
        }
    }

    /// A part of a second is dropped, not rounded.
    #[test]
    fn pinned_the_duration_drops_a_part_of_a_second() {
        for (started, now, expected) in [
            (
                "2026-11-20T12:00:00Z",
                "2026-11-20T12:01:29.999999999Z",
                "~1m 29s",
            ),
            (
                "2026-11-20T12:00:00.5Z",
                "2026-11-20T12:01:30.4Z",
                "~1m 29s",
            ),
            (
                "2026-11-20T12:00:00.5Z",
                "2026-11-20T12:01:30.5Z",
                "~1m 30s",
            ),
            (
                "2026-11-20T12:00:00.000000001Z",
                "2026-11-20T12:00:01Z",
                "~0s",
            ),
            // Digits after the ninth are not used.
            (
                "2026-11-20T12:00:00.0000000009Z",
                "2026-11-20T12:00:01Z",
                "~1s",
            ),
        ] {
            assert_eq!(
                running(started, now).as_deref(),
                Some(expected),
                "started {started:?}, current time {now:?}"
            );
        }
    }

    #[test]
    fn pinned_a_start_time_in_the_future_gives_zero() {
        for started in [
            "2026-11-20T12:00:00.000000001Z",
            "2026-11-20T12:00:01Z",
            "2026-11-21T12:00:00Z",
            "2026-11-20T12:00:01+00:00",
            "2026-11-20T12:00:00-00:01",
            "9999-12-31T23:59:59Z",
            "9999-12-31T23:59:59-23:59",
        ] {
            assert_eq!(
                running(started, STEP_NOW).as_deref(),
                Some("~0s"),
                "started {started:?}"
            );
        }
    }

    #[test]
    fn pinned_an_invalid_start_time_gives_no_duration() {
        for started in [
            "",
            "not-a-date",
            "2026-11-20",
            "2026-11-20T11:58:30",
            "2026-11-20T11:58Z",
            "2026-11-20T11:58:30+0000",
            "2026-11-20T11:58:30Z ",
            " 2026-11-20T11:58:30Z",
            "2026-11-31T11:58:30Z",
            "2026-11-20T24:00:00Z",
            "2026-11-20T11:58:30+00:00[UTC]",
            "20261120T115830Z",
        ] {
            assert_eq!(running(started, STEP_NOW), None, "started {started:?}");
        }
    }

    /// A leap second (`:60`) is read as second 59 when the time of day of the
    /// current time (UTC) is later than second 59. If not, it is read as the
    /// first second of the next minute.
    #[test]
    fn pinned_a_leap_second_in_the_start_time() {
        for (started, now, expected) in [
            // Current time of day later: 11:59:60 is read as 11:59:59.
            ("2026-11-20T11:59:60Z", "2026-11-20T12:00:10Z", "~11s"),
            ("2026-11-20T11:59:60.5Z", "2026-11-20T12:00:10Z", "~10s"),
            ("2026-11-20T11:59:60Z", "2026-11-20T12:00:00Z", "~1s"),
            (
                "2026-11-19T11:59:60Z",
                "2026-11-20T12:00:10Z",
                "~24h 0m 11s",
            ),
            // Same second of the day: read as 12:00:00.
            ("2026-11-20T11:59:60Z", "2026-11-20T11:59:59.900Z", "~0s"),
            (
                "2026-11-19T11:59:60Z",
                "2026-11-20T11:59:59.900Z",
                "~23h 59m 59s",
            ),
            // Current time of day earlier: read as 12:00:00.
            (
                "2026-11-19T11:59:60Z",
                "2026-11-20T11:59:50Z",
                "~23h 59m 50s",
            ),
            (
                "2026-11-19T11:59:60.5Z",
                "2026-11-20T11:59:50Z",
                "~23h 59m 49s",
            ),
            // With an offset, the time of day in UTC decides.
            ("2026-11-20T19:59:60+08:00", "2026-11-20T12:00:10Z", "~11s"),
            (
                "2026-11-19T19:59:60+08:00",
                "2026-11-20T11:59:50Z",
                "~23h 59m 50s",
            ),
        ] {
            assert_eq!(
                running(started, now).as_deref(),
                Some(expected),
                "started {started:?}, current time {now:?}"
            );
        }
    }

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
