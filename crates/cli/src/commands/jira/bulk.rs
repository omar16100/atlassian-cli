use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use atlassian_cli_api::path::encode_path_segment;
use atlassian_cli_api::ApiClient;
use atlassian_cli_bulk::BulkExecutor;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::adf::markdown_to_adf;
use super::issues::check_field_collisions;
use super::utils::JiraContext;
use crate::commands::common::{confirm_destructive, render_success, ExportFormat, MutationResult};

/// Build the POST body for a single bulk-import row. Pure, testable.
pub(crate) fn build_bulk_payload(project: &str, issue: &ImportIssue) -> Result<Value> {
    let empty = HashMap::new();
    let custom = issue.custom_fields.as_ref().unwrap_or(&empty);

    check_field_collisions(
        custom,
        &[
            ("project", "file project"),
            ("issuetype", "row.issue_type"),
            ("summary", "row.summary"),
        ],
        &[
            (
                "description",
                "row.description",
                issue.description.is_some(),
            ),
            ("assignee", "row.assignee", issue.assignee.is_some()),
            ("priority", "row.priority", issue.priority.is_some()),
            ("labels", "row.labels", !issue.labels.is_empty()),
        ],
    )?;

    let mut fields = json!({
        "project": { "key": project },
        "issuetype": { "name": issue.issue_type },
        "summary": issue.summary,
    });

    if let Some(desc) = &issue.description {
        fields["description"] = markdown_to_adf(desc);
    }
    if let Some(assignee) = &issue.assignee {
        fields["assignee"] = json!({ "id": assignee });
    }
    if let Some(priority) = &issue.priority {
        fields["priority"] = json!({ "name": priority });
    }
    if !issue.labels.is_empty() {
        fields["labels"] = json!(issue.labels);
    }
    for (key, value) in custom {
        fields[key] = value.clone();
    }

    Ok(json!({ "fields": fields }))
}

// Bulk transition issues
//
// Lists the matching issues and the transition each would run by default.
// `--execute` applies it, after the match count is typed back or `--yes` is
// given. The transition is resolved per issue before anything is asked or sent,
// so what is previewed and confirmed is what runs.
pub async fn bulk_transition(
    ctx: &JiraContext<'_>,
    jql: &str,
    transition: &str,
    execute: bool,
    assume_yes: bool,
    concurrency: usize,
) -> Result<()> {
    // Search for issues
    let issue_keys = search_issue_keys(ctx, jql).await?;

    if issue_keys.is_empty() {
        return render_no_matches(ctx);
    }

    eprintln!("Found {} issues to transition", issue_keys.len());

    // Transition IDs are only unique within one workflow, so the transition is
    // resolved against each issue's own list, never taken from the first issue.
    let plans = plan_transitions(ctx, &issue_keys, transition, concurrency).await?;
    let skipped = plans.iter().filter(|plan| plan.outcome.is_err()).count();

    if !execute {
        ctx.renderer.render_list(&transition_rows(&plans))?;
        // stderr, so a piped `-f json` listing stays machine-readable.
        eprintln!(
            "Listing only. {} issue(s) match, the transition cannot run on {skipped} \
             (not available or ambiguous). Pass --execute to apply it.",
            plans.len()
        );
        return Ok(());
    }

    // Confirm before the first request, not after some of them have happened.
    // With nothing to run there is nothing to confirm: the errors are reported.
    if !assume_yes && skipped < plans.len() {
        confirm_destructive(
            &plans.len().to_string(),
            &format!(
                "About to transition {} of {} issue(s) with {transition:?}.\n\
                 The transition cannot run on {skipped} issue(s) (not available or ambiguous), \
                 which are skipped and reported as errors.\n\
                 Re-run without --execute to list them first.",
                plans.len() - skipped,
                plans.len()
            ),
        )?;
    }

    apply_transitions(ctx, plans, concurrency).await?;

    render_success(
        ctx.renderer,
        "✅ Bulk transition completed",
        &MutationResult::new("Bulk transition completed"),
    )
}

#[derive(Serialize)]
struct AssignRow<'a> {
    key: &'a str,
    assignee: &'a str,
    action: &'static str,
}

// Bulk assign issues
//
// Lists the matching issues by default. `--execute` overwrites their assignee,
// after the match count is typed back or `--yes` is given.
pub async fn bulk_assign(
    ctx: &JiraContext<'_>,
    jql: &str,
    assignee: &str,
    execute: bool,
    assume_yes: bool,
    concurrency: usize,
) -> Result<()> {
    let issue_keys = search_issue_keys(ctx, jql).await?;

    if issue_keys.is_empty() {
        return render_no_matches(ctx);
    }

    eprintln!("Found {} issues to assign", issue_keys.len());

    if !execute {
        let rows: Vec<AssignRow<'_>> = issue_keys
            .iter()
            .map(|key| AssignRow {
                key: key.as_str(),
                assignee,
                action: "would assign",
            })
            .collect();
        ctx.renderer.render_list(&rows)?;
        // stderr, so a piped `-f json` listing stays machine-readable.
        eprintln!(
            "Listing only. {} issue(s) match. Pass --execute to assign them.",
            issue_keys.len()
        );
        return Ok(());
    }

    if !assume_yes {
        confirm_destructive(
            &issue_keys.len().to_string(),
            &format!(
                "About to assign {} issue(s) to {assignee}.\n\
                 Any current assignee is overwritten.\n\
                 Re-run without --execute to list them first.",
                issue_keys.len()
            ),
        )?;
    }

    let executor = BulkExecutor::new(concurrency, false);
    let client = ctx.client.clone();
    let assignee = assignee.to_string();

    executor
        .run(issue_keys, move |key| {
            let client = client.clone();
            let assignee = assignee.clone();
            async move {
                let payload = json!({ "accountId": assignee });
                let _: Value = client
                    .put(
                        &format!("/rest/api/3/issue/{}/assignee", encode_path_segment(&key)?),
                        &payload,
                    )
                    .await
                    .with_context(|| format!("Failed to assign issue {key}"))?;
                tracing::info!(%key, %assignee, "Assigned successfully");
                Ok(())
            }
        })
        .await?;

    render_success(
        ctx.renderer,
        "✅ Bulk assign completed",
        &MutationResult::new("Bulk assign completed"),
    )
}

// Bulk label operations
//
// Lists the matching issues by default. `--execute` applies the change, after
// the match count is typed back or `--yes` is given. At least one label is
// required for every action, so a missing value cannot turn `set` into "clear
// all labels".
pub async fn bulk_label(
    ctx: &JiraContext<'_>,
    jql: &str,
    action: LabelAction,
    labels: Vec<String>,
    execute: bool,
    assume_yes: bool,
    concurrency: usize,
) -> Result<()> {
    let labels = normalize_labels(labels)?;
    let issue_keys = search_issue_keys(ctx, jql).await?;

    if issue_keys.is_empty() {
        return render_no_matches(ctx);
    }

    eprintln!("Found {} issues to label", issue_keys.len());

    if !execute {
        let joined = labels.join(",");
        let rows: Vec<LabelRow<'_>> = issue_keys
            .iter()
            .map(|key| LabelRow {
                key: key.as_str(),
                action: format!("would {} labels", action.verb()),
                labels: joined.as_str(),
            })
            .collect();
        ctx.renderer.render_list(&rows)?;
        // stderr, so a piped `-f json` listing stays machine-readable.
        eprintln!(
            "Listing only. {} issue(s) match. Pass --execute to apply the change.",
            issue_keys.len()
        );
        return Ok(());
    }

    if !assume_yes {
        let effect = match action {
            LabelAction::Add | LabelAction::Remove => "",
            LabelAction::Set => "\nAll other labels on these issues are removed.",
        };
        confirm_destructive(
            &issue_keys.len().to_string(),
            &format!(
                "About to {} labels [{}] on {} issue(s).{effect}\n\
                 Re-run without --execute to list them first.",
                action.verb(),
                labels.join(", "),
                issue_keys.len()
            ),
        )?;
    }

    let executor = BulkExecutor::new(concurrency, false);
    let client = ctx.client.clone();

    executor
        .run(issue_keys, move |key| {
            let client = client.clone();
            let labels = labels.clone();
            let action = action.clone();
            async move {
                // Get current labels
                let issue: IssueWithLabels = client
                    .get(&format!("/rest/api/3/issue/{key}?fields=labels"))
                    .await
                    .with_context(|| format!("Failed to get issue {key}"))?;

                let new_labels = match action {
                    LabelAction::Add => {
                        let mut current = issue.fields.labels;
                        for label in labels {
                            if !current.contains(&label) {
                                current.push(label);
                            }
                        }
                        current
                    }
                    LabelAction::Remove => issue
                        .fields
                        .labels
                        .into_iter()
                        .filter(|l| !labels.contains(l))
                        .collect(),
                    LabelAction::Set => labels,
                };

                let payload = json!({ "fields": { "labels": new_labels } });
                let _: Value = client
                    .put(
                        &format!("/rest/api/3/issue/{}", encode_path_segment(&key)?),
                        &payload,
                    )
                    .await
                    .with_context(|| format!("Failed to update labels for {key}"))?;

                tracing::info!(%key, "Labels updated successfully");
                Ok(())
            }
        })
        .await?;

    render_success(
        ctx.renderer,
        "✅ Bulk label operation completed",
        &MutationResult::new("Bulk label operation completed"),
    )
}

// Bulk export issues
pub async fn bulk_export(
    ctx: &JiraContext<'_>,
    jql: &str,
    output: &PathBuf,
    format: ExportFormat,
    fields: Vec<String>,
) -> Result<()> {
    // Search for issues with specified fields
    let field_list: Vec<String> = if fields.is_empty() {
        vec!["*all".to_string()]
    } else {
        fields
    };

    #[derive(Deserialize)]
    struct SearchResponse {
        issues: Vec<Value>,
    }

    let payload = json!({
        "jql": jql,
        "maxResults": 1000,
        "fields": field_list,
    });

    let response: SearchResponse = ctx
        .client
        .post("/rest/api/3/search/jql", &payload)
        .await
        .context("Failed to search issues")?;

    if response.issues.is_empty() {
        println!("No issues found matching query");
        return Ok(());
    }

    println!("Found {} issues to export", response.issues.len());

    match format {
        ExportFormat::Json => {
            let json_str = serde_json::to_string_pretty(&response.issues)?;
            fs::write(output, json_str)?;
        }
        ExportFormat::Csv => {
            // Extract common fields for CSV
            let mut wtr = csv::Writer::from_path(output)?;

            // Write header
            wtr.write_record([
                "key", "summary", "status", "assignee", "reporter", "created",
            ])?;

            // Write rows
            for issue in &response.issues {
                let key = issue.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let summary = issue
                    .get("fields")
                    .and_then(|f| f.get("summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status = issue
                    .get("fields")
                    .and_then(|f| f.get("status"))
                    .and_then(|s| s.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let assignee = issue
                    .get("fields")
                    .and_then(|f| f.get("assignee"))
                    .and_then(|a| a.get("displayName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let reporter = issue
                    .get("fields")
                    .and_then(|f| f.get("reporter"))
                    .and_then(|r| r.get("displayName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let created = issue
                    .get("fields")
                    .and_then(|f| f.get("created"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                wtr.write_record([key, summary, status, assignee, reporter, created])?;
            }

            wtr.flush()?;
        }
    }

    println!(
        "✅ Exported {} issues to {}",
        response.issues.len(),
        output.display()
    );
    Ok(())
}

// Bulk import issues
pub async fn bulk_import(
    ctx: &JiraContext<'_>,
    file: &PathBuf,
    project: &str,
    dry_run: bool,
    concurrency: usize,
) -> Result<()> {
    let content = fs::read_to_string(file)?;
    let issues: Vec<ImportIssue> = serde_json::from_str(&content)?;

    if issues.is_empty() {
        println!("No issues to import from file");
        return Ok(());
    }

    println!("Found {} issues to import", issues.len());

    if dry_run {
        println!("🔍 Dry run mode - no changes will be made:");
        for (idx, issue) in issues.iter().enumerate() {
            println!("  Would create: {} - {}", idx + 1, issue.summary);
        }
        return Ok(());
    }

    let issues = resolve_import_assignees(&ctx.client, issues).await?;

    let executor = BulkExecutor::new(concurrency, dry_run);
    let client = ctx.client.clone();
    let project = project.to_string();

    executor
        .run(issues, move |issue| {
            let client = client.clone();
            let project = project.clone();
            async move {
                let payload = build_bulk_payload(&project, &issue)?;

                let response: CreateResponse = client
                    .post("/rest/api/3/issue", &payload)
                    .await
                    .context("Failed to create issue")?;

                tracing::info!(key = %response.key, "Issue created successfully");
                Ok(())
            }
        })
        .await?;

    render_success(
        ctx.renderer,
        "✅ Bulk import completed",
        &MutationResult::new("Bulk import completed"),
    )
}

// Helper functions

/// Replaces each `assignee` of an import file with its account ID, before any
/// issue is created. Each different value makes one lookup at most.
async fn resolve_import_assignees(
    client: &ApiClient,
    mut issues: Vec<ImportIssue>,
) -> Result<Vec<ImportIssue>> {
    let mut resolved: HashMap<String, String> = HashMap::new();
    for issue in &mut issues {
        let Some(value) = issue.assignee.as_ref() else {
            continue;
        };
        let account_id = match resolved.get(value) {
            Some(account_id) => account_id.clone(),
            None => {
                let account_id = super::users::resolve_account_id(client, value)
                    .await
                    .with_context(|| format!("Row '{}': assignee '{value}'", issue.summary))?;
                resolved.insert(value.clone(), account_id.clone());
                account_id
            }
        };
        issue.assignee = Some(account_id);
    }
    Ok(issues)
}

/// Nothing matched: the same rendering as a preview with no rows, so a machine
/// format gets an empty list rather than prose.
fn render_no_matches(ctx: &JiraContext<'_>) -> Result<()> {
    ctx.renderer
        .render_list_or_empty::<Value>(&[], "No issues found matching query")
}

async fn search_issue_keys(ctx: &JiraContext<'_>, jql: &str) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct SearchResponse {
        issues: Vec<Issue>,
    }

    #[derive(Deserialize)]
    struct Issue {
        key: String,
    }

    let payload = json!({
        "jql": jql,
        "maxResults": 1000,
        "fields": ["key"],
    });

    let response: SearchResponse = ctx
        .client
        .post("/rest/api/3/search/jql", &payload)
        .await
        .context("Failed to search issues")?;

    Ok(response.issues.into_iter().map(|i| i.key).collect())
}

/// A transition Jira offers for one issue (`GET .../issue/{key}/transitions`).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AvailableTransition {
    pub id: String,
    pub name: String,
    /// The status the issue lands in. Absent on some older instances.
    #[serde(default)]
    pub to: Option<TransitionTarget>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TransitionTarget {
    pub name: String,
}

/// What `--transition` means, fixed once so every issue is held to the same
/// meaning.
///
/// Transition IDs are only unique within one workflow: the same ID can be
/// "In Progress" in one project and "Done" in another. A name is the stable
/// handle across workflows, so an ID is only honoured on issues where it still
/// carries the name it had on the first issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransitionRequest {
    /// Matched by name (Unicode lowercase comparison) on each issue.
    ByName(String),
    /// An ID, pinned to the name it has on the first issue.
    ById { id: String, name: String },
    /// Applied to no issue, for the reason given. Used where the value cannot
    /// be tied to one transition without guessing.
    Unresolvable(String),
}

/// Name comparison by Unicode lowercase forms. `eq_ignore_ascii_case` would
/// treat `CAFÉ` and `café` as different names. This is not full case folding
/// (`Straße` and `STRASSE` differ), which fails closed: such a name is reported
/// as not available instead of matching the wrong transition.
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Transition IDs are numbers. A value made only of digits is meant as an ID.
fn looks_like_id(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_digit())
}

impl TransitionRequest {
    /// Classify `requested` against the transitions of the first issue.
    ///
    /// - An ID match pins the ID to the name it has there.
    /// - If the value is also the name of a *different* transition, it is
    ///   ambiguous and applied to no issue.
    /// - Without an ID match, a name match means a name, matched on every issue.
    /// - A value that matches nothing there is a name, unless it is all digits:
    ///   a number that is neither an ID nor a name on the first issue is applied
    ///   to no issue rather than looked for by name elsewhere.
    pub(crate) fn from_first_issue(first: &[AvailableTransition], requested: &str) -> Self {
        let by_id = first.iter().find(|t| t.id == requested);
        let named_elsewhere = |id: &str| {
            first
                .iter()
                .any(|t| t.id != id && same_name(&t.name, requested))
        };

        match by_id {
            Some(t) if named_elsewhere(&t.id) => Self::Unresolvable(format!(
                "transition {requested:?} is ambiguous: on the first issue it is the ID of \
                 {:?} and also the name of another transition, so it is applied to no issue. \
                 Narrow the query so the first issue does not offer both",
                t.name
            )),
            Some(t) => Self::ById {
                id: t.id.clone(),
                name: t.name.clone(),
            },
            None if first.iter().any(|t| same_name(&t.name, requested)) => {
                Self::ByName(requested.to_string())
            }
            None if looks_like_id(requested) => Self::Unresolvable(format!(
                "transition {requested:?} is neither the ID nor the name of a transition on \
                 the first issue, so the number is applied to no issue. Use the transition name"
            )),
            None => Self::ByName(requested.to_string()),
        }
    }

    /// The request when the first issue's transitions could not be read.
    ///
    /// A name does not depend on the first issue and is resolved on each issue.
    /// An ID does, so it stays closed: nothing runs on a value whose meaning is
    /// unknown.
    pub(crate) fn for_unreadable_first_issue(requested: &str) -> Self {
        if looks_like_id(requested) {
            Self::Unresolvable(format!(
                "transition {requested:?} looks like an ID, but the transitions of the first \
                 issue could not be read, so its meaning is unknown and it is applied to no issue"
            ))
        } else {
            Self::ByName(requested.to_string())
        }
    }
}

/// The transition that would run on one issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTransition {
    pub id: String,
    pub name: String,
    /// Name of the status the issue ends up in, when Jira reports it.
    pub to_status: Option<String>,
}

/// Why the requested transition does not run on one issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unresolved {
    /// No transition on the issue matches the request.
    NotAvailable,
    /// More than one transition matches, as `(id, name)` sorted by ID. None of
    /// them is chosen.
    Ambiguous(Vec<(String, String)>),
}

/// Find the transition for one issue.
///
/// There is no fallback and no guess: an issue that does not offer the
/// requested transition is left alone rather than given a similar one, and an
/// issue where several transitions match (for example `Done` and `DONE`) is
/// left alone rather than given whichever Jira happened to list first. The
/// result therefore does not depend on the order of Jira's response.
pub(crate) fn resolve_transition(
    available: &[AvailableTransition],
    request: &TransitionRequest,
) -> Result<ResolvedTransition, Unresolved> {
    let matches: Vec<&AvailableTransition> = match request {
        TransitionRequest::ByName(name) => available
            .iter()
            .filter(|t| same_name(&t.name, name))
            .collect(),
        TransitionRequest::ById { id, name } => available
            .iter()
            .filter(|t| t.id == *id && same_name(&t.name, name))
            .collect(),
        TransitionRequest::Unresolvable(_) => Vec::new(),
    };

    match matches.as_slice() {
        [] => Err(Unresolved::NotAvailable),
        [found] => Ok(ResolvedTransition {
            id: found.id.clone(),
            name: found.name.clone(),
            to_status: found.to.as_ref().map(|to| to.name.clone()),
        }),
        several => {
            let mut candidates: Vec<(String, String)> = several
                .iter()
                .map(|t| (t.id.clone(), t.name.clone()))
                .collect();
            candidates.sort();
            Err(Unresolved::Ambiguous(candidates))
        }
    }
}

fn unavailable_message(key: &str, request: &TransitionRequest) -> String {
    match request {
        TransitionRequest::ByName(name) => {
            format!("{key}: transition {name:?} is not available on this issue")
        }
        TransitionRequest::ById { id, name } => format!(
            "{key}: transition ID {id} ({name:?} on the first issue) is not available on this \
             issue, the ID is missing or belongs to a different transition here"
        ),
        TransitionRequest::Unresolvable(reason) => format!("{key}: {reason}"),
    }
}

fn ambiguous_message(
    key: &str,
    request: &TransitionRequest,
    candidates: &[(String, String)],
) -> String {
    let requested = match request {
        TransitionRequest::ByName(name) => name.as_str(),
        TransitionRequest::ById { id, .. } => id.as_str(),
        TransitionRequest::Unresolvable(_) => "",
    };
    let listed = candidates
        .iter()
        .map(|(id, name)| format!("{id} {name:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{key}: transition {requested:?} is ambiguous on this issue, it matches {} transitions \
         ({listed}), so none is run. Pass the ID of the intended transition, or narrow the query",
        candidates.len()
    )
}

/// Why an issue gets no transition request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Skipped {
    /// Several transitions matched, as opposed to none or a failed lookup.
    pub ambiguous: bool,
    /// The error reported for the issue, starting with its key.
    pub message: String,
}

impl Skipped {
    fn not_available(message: String) -> Self {
        Self {
            ambiguous: false,
            message,
        }
    }
}

/// The outcome of resolving the requested transition for one issue.
#[derive(Debug)]
pub(crate) struct IssuePlan {
    pub key: String,
    /// The transition to run, or why there is none.
    pub outcome: Result<ResolvedTransition, Skipped>,
}

impl IssuePlan {
    fn for_issue(
        key: String,
        available: &[AvailableTransition],
        request: &TransitionRequest,
    ) -> Self {
        let outcome =
            resolve_transition(available, request).map_err(|unresolved| match unresolved {
                Unresolved::NotAvailable => {
                    Skipped::not_available(unavailable_message(&key, request))
                }
                Unresolved::Ambiguous(candidates) => Skipped {
                    ambiguous: true,
                    message: ambiguous_message(&key, request, &candidates),
                },
            });
        Self { key, outcome }
    }

    fn failed(key: String, message: String) -> Self {
        Self {
            key,
            outcome: Err(Skipped::not_available(message)),
        }
    }
}

async fn fetch_transitions(client: &ApiClient, key: &str) -> Result<Vec<AvailableTransition>> {
    #[derive(Deserialize)]
    struct TransitionsResponse {
        transitions: Vec<AvailableTransition>,
    }

    let available: TransitionsResponse = client
        .get(&format!(
            "/rest/api/3/issue/{}/transitions",
            encode_path_segment(key)?
        ))
        .await
        .with_context(|| format!("Failed to get transitions for {key}"))?;
    Ok(available.transitions)
}

/// Resolve the requested transition for every issue, in the order of `issue_keys`.
///
/// Only reads. The first issue decides what an ID means (see
/// [`TransitionRequest`]), so its transitions are fetched before the rest, which
/// are fetched concurrently. A failed read never aborts: it is that issue's
/// outcome. If it is the first issue, a name is still resolved on every other
/// issue, while an ID-like value stays closed and applies to no issue. When the
/// request is unresolvable the other issues are not read at all.
async fn plan_transitions(
    ctx: &JiraContext<'_>,
    issue_keys: &[String],
    requested: &str,
    concurrency: usize,
) -> Result<Vec<IssuePlan>> {
    let Some((first_key, rest)) = issue_keys.split_first() else {
        return Ok(Vec::new());
    };

    let (first_plan, request) = match fetch_transitions(&ctx.client, first_key).await {
        Ok(available) => {
            let request = TransitionRequest::from_first_issue(&available, requested);
            let plan = IssuePlan::for_issue(first_key.clone(), &available, &request);
            (plan, request)
        }
        Err(err) => (
            IssuePlan::failed(first_key.clone(), format!("{err:#}")),
            TransitionRequest::for_unreadable_first_issue(requested),
        ),
    };

    let mut plans = Vec::with_capacity(issue_keys.len());
    plans.push(first_plan);

    if matches!(request, TransitionRequest::Unresolvable(_)) {
        plans.extend(
            rest.iter()
                .map(|key| IssuePlan::failed(key.clone(), unavailable_message(key, &request))),
        );
        return Ok(plans);
    }

    let client = ctx.client.clone();
    let job_request = request.clone();
    let items: Vec<(usize, String)> = rest.iter().cloned().enumerate().collect();

    let result = BulkExecutor::new(concurrency, false)
        .with_progress(false)
        .execute_with_results(items, move |(idx, key): (usize, String)| {
            let client = client.clone();
            let request = job_request.clone();
            async move {
                let plan = match fetch_transitions(&client, &key).await {
                    Ok(available) => IssuePlan::for_issue(key, &available, &request),
                    Err(err) => IssuePlan::failed(key, format!("{err:#}")),
                };
                Ok((idx, plan))
            }
        })
        .await?;

    if let Some((_, err)) = result.failed.into_iter().next() {
        return Err(err);
    }

    let mut resolved = result.successful;
    resolved.sort_by_key(|(idx, _)| *idx);
    plans.extend(resolved.into_iter().map(|(_, plan)| plan));
    Ok(plans)
}

#[derive(Serialize)]
struct TransitionRow<'a> {
    key: &'a str,
    transition: &'a str,
    to_status: &'a str,
    action: &'a str,
}

/// One row per issue showing what would run, or that nothing would.
fn transition_rows(plans: &[IssuePlan]) -> Vec<TransitionRow<'_>> {
    plans
        .iter()
        .map(|plan| match &plan.outcome {
            Ok(resolved) => TransitionRow {
                key: plan.key.as_str(),
                transition: resolved.name.as_str(),
                to_status: resolved.to_status.as_deref().unwrap_or(""),
                action: "would transition",
            },
            Err(skipped) => TransitionRow {
                key: plan.key.as_str(),
                transition: "",
                to_status: "",
                action: if skipped.ambiguous {
                    "ambiguous"
                } else {
                    "not available"
                },
            },
        })
        .collect()
}

/// Run the resolved transitions. Issues without one are reported as errors.
///
/// Every failure is printed with the issue key, issues that were transitioned
/// stay transitioned, and the call fails when any issue was not.
async fn apply_transitions(
    ctx: &JiraContext<'_>,
    plans: Vec<IssuePlan>,
    concurrency: usize,
) -> Result<()> {
    let mut problems: Vec<String> = Vec::new();
    let mut jobs: Vec<(String, ResolvedTransition)> = Vec::new();
    for plan in plans {
        match plan.outcome {
            Ok(resolved) => jobs.push((plan.key, resolved)),
            Err(skipped) => problems.push(skipped.message),
        }
    }
    let total = jobs.len() + problems.len();

    let client = ctx.client.clone();
    let result = BulkExecutor::new(concurrency, false)
        .execute_with_results(
            jobs,
            move |(key, resolved): (String, ResolvedTransition)| {
                let client = client.clone();
                async move {
                    let payload = json!({ "transition": { "id": resolved.id } });
                    let _: Value = client
                        .post(
                            &format!(
                                "/rest/api/3/issue/{}/transitions",
                                encode_path_segment(&key)?
                            ),
                            &payload,
                        )
                        .await
                        .with_context(|| {
                            format!("Failed to transition issue {key} via {:?}", resolved.name)
                        })?;
                    tracing::info!(%key, transition = %resolved.name, "Transitioned successfully");
                    Ok(key)
                }
            },
        )
        .await?;

    let transitioned = result.success_count();
    problems.extend(result.failed.iter().map(|(_, err)| format!("{err:#}")));

    if !problems.is_empty() {
        eprintln!("Not transitioned:");
        for problem in &problems {
            eprintln!("  {problem}");
        }
        bail!(
            "{} of {total} issue(s) were not transitioned ({transitioned} transitioned)",
            problems.len()
        );
    }

    Ok(())
}

// Data structures

#[derive(Debug, Clone)]
pub enum LabelAction {
    Add,
    Remove,
    Set,
}

impl LabelAction {
    fn verb(&self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Set => "set",
        }
    }
}

#[derive(Serialize)]
struct LabelRow<'a> {
    key: &'a str,
    action: String,
    labels: &'a str,
}

/// Trim the labels and drop empty values, and refuse an empty result.
///
/// `set` with no labels used to send `labels: []`, which clears every label on
/// every matching issue. A missing or empty list is now rejected for every
/// action, so a mistyped or empty variable cannot clear labels implicitly.
/// `remove` with explicit labels is unchanged.
pub(crate) fn normalize_labels(labels: Vec<String>) -> Result<Vec<String>> {
    let labels: Vec<String> = labels
        .into_iter()
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .collect();

    if labels.is_empty() {
        bail!("--labels needs at least one non-empty label, for every action.");
    }
    Ok(labels)
}

#[derive(Deserialize)]
struct IssueWithLabels {
    fields: LabelsField,
}

#[derive(Deserialize)]
struct LabelsField {
    labels: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ImportIssue {
    pub summary: String,
    pub issue_type: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub custom_fields: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
struct CreateResponse {
    key: String,
}

#[cfg(test)]
mod transition_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use atlassian_cli_output::{OutputFormat, OutputRenderer};
    use serde_json::json;
    use wiremock::matchers::{method, path as path_matcher};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_import_issue_deserialize_with_custom_fields() {
        let json_str = r#"{
            "summary": "Test ticket",
            "issue_type": "Task",
            "custom_fields": {
                "customfield_10001": {"value": "Alpha"},
                "customfield_10002": [{"value": "Beta"}]
            }
        }"#;
        let issue: ImportIssue = serde_json::from_str(json_str).unwrap();
        assert_eq!(issue.summary, "Test ticket");
        let cf = issue.custom_fields.unwrap();
        assert_eq!(cf["customfield_10001"], json!({"value": "Alpha"}));
        assert_eq!(cf["customfield_10002"], json!([{"value": "Beta"}]));
    }

    #[tokio::test]
    async fn import_assignees_resolve_with_one_lookup_for_each_value() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_matcher("/rest/api/3/user/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"accountId": "acc-1", "displayName": "Dev One", "emailAddress": "dev@example.com"}
            ])))
            .mount(&server)
            .await;
        let row = |assignee: Option<&str>| ImportIssue {
            summary: "Row".to_string(),
            issue_type: "Task".to_string(),
            description: None,
            assignee: assignee.map(str::to_string),
            priority: None,
            labels: vec![],
            custom_fields: None,
        };
        let issues = vec![
            row(Some("dev@example.com")),
            row(Some("acc-9")),
            row(None),
            row(Some("dev@example.com")),
        ];
        let client = ApiClient::new(server.uri()).unwrap();

        let resolved = resolve_import_assignees(&client, issues).await.unwrap();

        let assignees: Vec<Option<&str>> = resolved.iter().map(|i| i.assignee.as_deref()).collect();
        assert_eq!(
            assignees,
            vec![Some("acc-1"), Some("acc-9"), None, Some("acc-1")]
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[test]
    fn test_import_issue_deserialize_without_custom_fields() {
        let json_str = r#"{
            "summary": "Basic ticket",
            "issue_type": "Bug"
        }"#;
        let issue: ImportIssue = serde_json::from_str(json_str).unwrap();
        assert_eq!(issue.summary, "Basic ticket");
        assert!(issue.custom_fields.is_none());
    }

    #[test]
    fn test_import_issue_deserialize_empty_custom_fields() {
        let json_str = r#"{
            "summary": "Empty customs",
            "issue_type": "Story",
            "custom_fields": {}
        }"#;
        let issue: ImportIssue = serde_json::from_str(json_str).unwrap();
        assert!(issue.custom_fields.unwrap().is_empty());
    }

    #[test]
    fn test_import_issue_full_roundtrip() {
        let json_str = r#"{
            "summary": "Full ticket",
            "issue_type": "Task",
            "description": "Some description",
            "assignee": "user123",
            "priority": "High",
            "labels": ["backend", "urgent"],
            "custom_fields": {
                "customfield_10001": "plain string",
                "customfield_10002": 42
            }
        }"#;
        let issue: ImportIssue = serde_json::from_str(json_str).unwrap();
        assert_eq!(issue.summary, "Full ticket");
        assert_eq!(issue.description.as_deref(), Some("Some description"));
        assert_eq!(issue.assignee.as_deref(), Some("user123"));
        assert_eq!(issue.priority.as_deref(), Some("High"));
        assert_eq!(issue.labels, vec!["backend", "urgent"]);
        let cf = issue.custom_fields.unwrap();
        assert_eq!(cf["customfield_10001"], json!("plain string"));
        assert_eq!(cf["customfield_10002"], json!(42));
    }

    fn minimal_issue() -> ImportIssue {
        ImportIssue {
            summary: "s".to_string(),
            issue_type: "Task".to_string(),
            description: None,
            assignee: None,
            priority: None,
            labels: vec![],
            custom_fields: None,
        }
    }

    #[test]
    fn build_bulk_payload_merges_typed_and_custom() {
        let mut issue = minimal_issue();
        issue.description = Some("d".into());
        issue.labels = vec!["backend".into()];
        issue.custom_fields = Some(
            [("customfield_10010".to_string(), json!({"value": "x"}))]
                .into_iter()
                .collect(),
        );
        let payload = build_bulk_payload("PROJ", &issue).unwrap();
        let fields = &payload["fields"];
        assert_eq!(fields["project"]["key"], "PROJ");
        assert_eq!(fields["issuetype"]["name"], "Task");
        assert_eq!(fields["summary"], "s");
        assert_eq!(fields["labels"], json!(["backend"]));
        assert_eq!(fields["customfield_10010"], json!({"value": "x"}));
        assert_eq!(fields["description"]["type"], "doc");
    }

    #[test]
    fn build_bulk_payload_rejects_mandatory_collision() {
        let mut issue = minimal_issue();
        issue.custom_fields = Some(
            [("summary".to_string(), json!("raw"))]
                .into_iter()
                .collect(),
        );
        let err = build_bulk_payload("PROJ", &issue).unwrap_err().to_string();
        assert!(err.contains("reserved"), "got: {err}");
        assert!(err.contains("row.summary"), "got: {err}");
    }

    #[test]
    fn build_bulk_payload_rejects_labels_collision_when_both_set() {
        let mut issue = minimal_issue();
        issue.labels = vec!["a".into()];
        issue.custom_fields = Some([("labels".to_string(), json!(["b"]))].into_iter().collect());
        let err = build_bulk_payload("PROJ", &issue).unwrap_err().to_string();
        assert!(err.contains("collides"), "got: {err}");
        assert!(err.contains("row.labels"), "got: {err}");
    }

    #[test]
    fn build_bulk_payload_allows_raw_only_labels() {
        let mut issue = minimal_issue();
        issue.custom_fields = Some(
            [("labels".to_string(), json!(["b", "c"]))]
                .into_iter()
                .collect(),
        );
        let payload = build_bulk_payload("PROJ", &issue).unwrap();
        assert_eq!(payload["fields"]["labels"], json!(["b", "c"]));
    }

    pub(super) async fn mount_search(server: &MockServer, keys: &[&str]) {
        let issues: Vec<_> = keys.iter().map(|k| json!({ "key": k })).collect();
        Mock::given(method("POST"))
            .and(path_matcher("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "issues": issues })))
            .mount(server)
            .await;
    }

    pub(super) fn jira_ctx<'a>(
        server: &MockServer,
        renderer: &'a OutputRenderer,
    ) -> JiraContext<'a> {
        JiraContext {
            client: ApiClient::new(server.uri()).unwrap(),
            renderer,
        }
    }

    /// Path and JSON body of every write received (the search that
    /// selects the issues is a POST too, and is not a write).
    async fn writes(server: &MockServer) -> Vec<(String, Value)> {
        let mut writes: Vec<(String, Value)> = server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| {
                (r.method == wiremock::http::Method::POST
                    || r.method == wiremock::http::Method::PUT)
                    && r.url.path() != "/rest/api/3/search/jql"
            })
            .map(|r| {
                (
                    r.url.path().to_string(),
                    serde_json::from_slice(&r.body).unwrap_or(Value::Null),
                )
            })
            .collect();
        writes.sort_by(|a, b| a.0.cmp(&b.0));
        writes
    }

    /// Two issues that accept an assignee change and a label update.
    async fn server_with_writable_issues() -> MockServer {
        writable_issue_server(None).await
    }

    /// The same server for runs that must not write: it verifies on drop that
    /// no PUT arrived.
    async fn read_only_issue_server() -> MockServer {
        writable_issue_server(Some(0)).await
    }

    async fn mount_put(server: &MockServer, request_path: String, puts: Option<u64>) {
        let mock = Mock::given(method("PUT"))
            .and(path_matcher(request_path))
            .respond_with(ResponseTemplate::new(204));
        match puts {
            Some(expected) => mock.expect(expected).mount(server).await,
            None => mock.mount(server).await,
        }
    }

    async fn writable_issue_server(puts: Option<u64>) -> MockServer {
        let server = MockServer::start().await;
        mount_search(&server, &["A-1", "B-1"]).await;
        for key in ["A-1", "B-1"] {
            mount_put(&server, format!("/rest/api/3/issue/{key}/assignee"), puts).await;
            Mock::given(method("GET"))
                .and(path_matcher(format!("/rest/api/3/issue/{key}")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({ "fields": { "labels": ["old"] } })),
                )
                .mount(&server)
                .await;
            mount_put(&server, format!("/rest/api/3/issue/{key}"), puts).await;
        }
        server
    }

    /// Nothing matches the JQL: no command writes. The empty list rendering is
    /// checked end to end in the e2e tests.
    #[tokio::test]
    async fn no_match_is_a_no_op_for_every_command() {
        let server = MockServer::start().await;
        mount_search(&server, &[]).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = jira_ctx(&server, &renderer);

        bulk_transition(&ctx, "project = X", "Done", false, false, 2)
            .await
            .unwrap();
        bulk_assign(&ctx, "project = X", "acc-1", true, true, 2)
            .await
            .unwrap();
        bulk_label(
            &ctx,
            "project = X",
            LabelAction::Add,
            vec!["a".into()],
            true,
            true,
            2,
        )
        .await
        .unwrap();

        assert!(writes(&server).await.is_empty());
    }

    #[tokio::test]
    async fn assign_lists_by_default_and_changes_nothing() {
        let server = read_only_issue_server().await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = jira_ctx(&server, &renderer);

        bulk_assign(&ctx, "project = X", "acc-1", false, false, 2)
            .await
            .unwrap();

        assert!(writes(&server).await.is_empty(), "listing must not write");
    }

    #[tokio::test]
    async fn assign_with_execute_and_yes_assigns_every_issue() {
        let server = server_with_writable_issues().await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = jira_ctx(&server, &renderer);

        bulk_assign(&ctx, "project = X", "acc-1", true, true, 2)
            .await
            .unwrap();

        let writes = writes(&server).await;
        assert_eq!(writes.len(), 2, "{writes:?}");
        assert_eq!(writes[0].0, "/rest/api/3/issue/A-1/assignee");
        assert_eq!(writes[1].0, "/rest/api/3/issue/B-1/assignee");
        assert!(writes
            .iter()
            .all(|(_, body)| body == &json!({ "accountId": "acc-1" })));
    }

    #[tokio::test]
    async fn label_lists_by_default_and_changes_nothing() {
        for action in [LabelAction::Add, LabelAction::Remove, LabelAction::Set] {
            let server = read_only_issue_server().await;
            let renderer = OutputRenderer::new(OutputFormat::Json);
            let ctx = jira_ctx(&server, &renderer);

            bulk_label(
                &ctx,
                "project = X",
                action.clone(),
                vec!["new".into()],
                false,
                false,
                2,
            )
            .await
            .unwrap();

            assert!(
                writes(&server).await.is_empty(),
                "{action:?}: listing must not write"
            );
        }
    }

    #[tokio::test]
    async fn label_set_with_execute_and_yes_writes_the_given_labels() {
        let server = server_with_writable_issues().await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = jira_ctx(&server, &renderer);

        bulk_label(
            &ctx,
            "project = X",
            LabelAction::Set,
            vec!["new".into(), " other ".into()],
            true,
            true,
            2,
        )
        .await
        .unwrap();

        let writes = writes(&server).await;
        assert_eq!(writes.len(), 2, "{writes:?}");
        assert!(writes
            .iter()
            .all(|(_, body)| body == &json!({ "fields": { "labels": ["new", "other"] } })));
    }

    /// `set` without labels used to send `labels: []` for every match. Now no
    /// action accepts an empty list, and nothing is requested, not even the
    /// search.
    #[tokio::test]
    async fn label_without_labels_is_an_error_and_sends_nothing() {
        let cases: [&[&str]; 3] = [&[], &[""], &[" "]];
        for case in cases {
            let labels: Vec<String> = case.iter().map(|label| label.to_string()).collect();
            for action in [LabelAction::Add, LabelAction::Remove, LabelAction::Set] {
                let server = read_only_issue_server().await;
                let renderer = OutputRenderer::new(OutputFormat::Json);
                let ctx = jira_ctx(&server, &renderer);

                let err = bulk_label(
                    &ctx,
                    "project = X",
                    action.clone(),
                    labels.clone(),
                    true,
                    true,
                    2,
                )
                .await
                .expect_err("empty labels must be rejected");

                assert!(
                    format!("{err:#}").contains("--labels"),
                    "{action:?}: {err:#}"
                );
                assert!(
                    server
                        .received_requests()
                        .await
                        .unwrap_or_default()
                        .is_empty(),
                    "{action:?} {labels:?}: no request may be sent"
                );
            }
        }
    }

    #[test]
    fn normalize_labels_trims_and_drops_empty_values() {
        let labels = normalize_labels(vec![" a ".into(), String::new(), "b".into()]).unwrap();
        assert_eq!(labels, vec!["a", "b"]);
        assert!(normalize_labels(vec![]).is_err());
        assert!(normalize_labels(vec!["  ".into()]).is_err());
    }
}
