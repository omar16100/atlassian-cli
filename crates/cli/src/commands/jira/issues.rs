use std::collections::HashMap;

use anyhow::{anyhow, bail, Context, Result};
use atlassian_cli_output::OutputFormat;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::adf::markdown_to_adf;
use super::attachments::{attachments_markdown, JiraAttachment};
use super::utils::JiraContext;
use crate::commands::common::{render_success, MutationResult};
use crate::query::JqlBuilder;

/// Parse `--field key=json_value` pairs into a HashMap. Rejects duplicate keys.
pub fn parse_custom_fields(raw: &[String]) -> Result<HashMap<String, Value>> {
    let mut map = HashMap::new();
    for entry in raw {
        let (key, val) = entry.split_once('=').ok_or_else(|| {
            anyhow!(
                "Invalid --field format '{}': expected key=JSON_VALUE",
                entry
            )
        })?;
        let parsed: Value = serde_json::from_str(val)
            .with_context(|| format!("Invalid JSON in --field '{}': {}", key, val))?;
        if map.insert(key.to_string(), parsed).is_some() {
            bail!("--field '{key}' specified more than once");
        }
    }
    Ok(map)
}

/// Hard-error if `custom` tries to set reserved or already-set typed fields.
///
/// `mandatory`: (jira_payload_key, cli_flag_hint) — always rejected.
/// `optional_set`: (jira_payload_key, cli_flag_hint, typed_is_set) — rejected
/// only when both the typed flag and the raw field target the same key.
pub(crate) fn check_field_collisions(
    custom: &HashMap<String, Value>,
    mandatory: &[(&str, &str)],
    optional_set: &[(&str, &str, bool)],
) -> Result<()> {
    for key in custom.keys() {
        if let Some((_, hint)) = mandatory.iter().find(|(k, _)| *k == key.as_str()) {
            bail!("--field cannot set reserved key '{key}'; use {hint} instead");
        }
        if let Some((_, hint, _)) = optional_set
            .iter()
            .find(|(k, _, set)| *k == key.as_str() && *set)
        {
            bail!("--field '{key}' collides with {hint} already set; pick one source");
        }
    }
    Ok(())
}

// Issue CRUD Operations

#[allow(clippy::too_many_arguments)]
pub async fn search_issues(
    ctx: &JiraContext<'_>,
    jql: Option<&str>,
    assignee: Option<&str>,
    status: &[String],
    priority: Option<&str>,
    label: &[String],
    r#type: Option<&str>,
    project: Option<&str>,
    text: Option<&str>,
    show_query: bool,
    limit: usize,
) -> Result<()> {
    // Build JQL from filters or use raw JQL
    let final_jql = if let Some(raw_jql) = jql {
        raw_jql.to_string()
    } else {
        // Build JQL from filter parameters
        let mut builder = JqlBuilder::new();

        if let Some(a) = assignee {
            builder = builder.eq("assignee", a);
        }
        if !status.is_empty() {
            builder = builder.in_list("status", status);
        }
        if let Some(p) = priority {
            builder = builder.eq("priority", p);
        }
        if !label.is_empty() {
            builder = builder.in_list("labels", label);
        }
        if let Some(t) = r#type {
            builder = builder.eq("type", t);
        }
        if let Some(proj) = project {
            builder = builder.eq("project", proj);
        }
        if let Some(txt) = text {
            builder = builder.contains("summary", txt);
        }

        let built_jql = builder.finish();
        if built_jql.is_empty() {
            return Err(anyhow!(
                "No search criteria provided. Use --jql or filter flags (--assignee, --status, etc.)"
            ));
        }
        built_jql
    };

    // Show query if requested
    if show_query {
        println!("JQL Query: {}", final_jql);
        if jql.is_none() {
            println!();
        }
    }

    #[derive(Deserialize)]
    struct SearchResponse {
        issues: Vec<Issue>,
        #[allow(dead_code)]
        #[serde(rename = "isLast")]
        is_last: Option<bool>,
        #[allow(dead_code)]
        #[serde(rename = "nextPageToken")]
        next_page_token: Option<String>,
    }

    let max_results = limit.min(1000);
    let query = format!(
        "/rest/api/3/search/jql?jql={}&maxResults={}&fields=key,summary,status,assignee,issuetype",
        urlencoding::encode(&final_jql),
        max_results
    );

    let response: SearchResponse = ctx
        .client
        .get(&query)
        .await
        .context("Failed to execute search")?;

    if response.issues.is_empty() {
        ctx.verify_auth().await?;
        tracing::info!("No issues found");
        println!("No issues found");
        return Ok(());
    }

    #[derive(Serialize)]
    struct Row<'a> {
        key: &'a str,
        summary: &'a str,
        status: &'a str,
        assignee: &'a str,
        issue_type: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .issues
        .iter()
        .map(|issue| Row {
            key: issue.key.as_str(),
            summary: issue.fields.summary.as_deref().unwrap_or(""),
            status: issue
                .fields
                .status
                .as_ref()
                .map(|s| s.name.as_str())
                .unwrap_or(""),
            assignee: issue
                .fields
                .assignee
                .as_ref()
                .map(|a| a.display_name.as_str())
                .unwrap_or(""),
            issue_type: issue
                .fields
                .issuetype
                .as_ref()
                .map(|t| t.name.as_str())
                .unwrap_or(""),
        })
        .collect();

    ctx.renderer.render(&rows)
}

pub async fn view_issue(ctx: &JiraContext<'_>, key: &str) -> Result<()> {
    let issue: Issue = ctx
        .client
        .get(&format!("/rest/api/3/issue/{key}"))
        .await
        .with_context(|| format!("Failed to fetch issue {key}"))?;

    if ctx.renderer.format() == OutputFormat::Markdown {
        let summary = issue.fields.summary.as_deref().unwrap_or("");
        let status = issue
            .fields
            .status
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("");
        let assignee = issue
            .fields
            .assignee
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("Unassigned");
        let reporter = issue
            .fields
            .reporter
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("");
        let issue_type = issue
            .fields
            .issuetype
            .as_ref()
            .map(|t| t.name.as_str())
            .unwrap_or("");
        let description = issue
            .fields
            .description
            .as_ref()
            .map(extract_adf_markdown)
            .unwrap_or_default();

        let attachments_md = attachments_markdown(&issue.fields.attachment);
        let sprint_md = issue
            .fields
            .sprint
            .as_ref()
            .and_then(extract_active_sprint)
            .map(|s| format!("| Sprint | {s} |\n"))
            .unwrap_or_default();

        let md = format!(
            "# {key}: {summary}\n\n\
             | Field | Value |\n\
             | --- | --- |\n\
             | Status | {status} |\n\
             | Type | {issue_type} |\n\
             | Assignee | {assignee} |\n\
             | Reporter | {reporter} |\n\
             {sprint_md}\n\
             ## Description\n\n\
             {description}{attachments_md}"
        );
        return ctx.renderer.render_raw(&md);
    }

    #[derive(Serialize)]
    struct IssueDetails<'a> {
        key: &'a str,
        summary: &'a str,
        status: &'a str,
        description: String,
        assignee: &'a str,
        reporter: &'a str,
        issue_type: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        sprint: Option<String>,
        attachments: &'a Vec<JiraAttachment>,
    }

    let view = IssueDetails {
        key: issue.key.as_str(),
        summary: issue.fields.summary.as_deref().unwrap_or(""),
        status: issue
            .fields
            .status
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or(""),
        sprint: issue.fields.sprint.as_ref().and_then(extract_active_sprint),
        description: issue
            .fields
            .description
            .as_ref()
            .map(extract_adf_text)
            .unwrap_or_default(),
        assignee: issue
            .fields
            .assignee
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or(""),
        reporter: issue
            .fields
            .reporter
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or(""),
        issue_type: issue
            .fields
            .issuetype
            .as_ref()
            .map(|t| t.name.as_str())
            .unwrap_or(""),
        attachments: &issue.fields.attachment,
    };

    ctx.renderer.render(&view)
}

/// Build the POST body for `POST /rest/api/3/issue`. Pure, testable.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_create_payload(
    project: &str,
    issue_type: &str,
    summary: &str,
    description: Option<&str>,
    assignee: Option<&str>,
    priority: Option<&str>,
    custom: &HashMap<String, Value>,
) -> Result<Value> {
    use serde_json::json;

    check_field_collisions(
        custom,
        &[
            ("project", "--project"),
            ("issuetype", "--issue-type"),
            ("summary", "--summary"),
        ],
        &[
            ("description", "--description", description.is_some()),
            ("assignee", "--assignee", assignee.is_some()),
            ("priority", "--priority", priority.is_some()),
        ],
    )?;

    let mut fields = json!({
        "project": { "key": project },
        "issuetype": { "name": issue_type },
        "summary": summary,
    });

    if let Some(desc) = description {
        fields["description"] = markdown_to_adf(desc);
    }
    if let Some(user) = assignee {
        fields["assignee"] = json!({ "id": user });
    }
    if let Some(pri) = priority {
        fields["priority"] = json!({ "name": pri });
    }
    for (key, value) in custom {
        fields[key] = value.clone();
    }

    Ok(json!({ "fields": fields }))
}

#[allow(clippy::too_many_arguments)]
pub async fn create_issue(
    ctx: &JiraContext<'_>,
    project: &str,
    issue_type: &str,
    summary: &str,
    description: Option<&str>,
    assignee: Option<&str>,
    priority: Option<&str>,
    custom_fields: &HashMap<String, Value>,
    sprint: Option<&str>,
) -> Result<()> {
    let payload = build_create_payload(
        project,
        issue_type,
        summary,
        description,
        assignee,
        priority,
        custom_fields,
    )?;

    #[derive(Deserialize)]
    struct CreateResponse {
        key: String,
        id: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/rest/api/3/issue", &payload)
        .await
        .context("Failed to create issue")?;

    if let Some(sprint_id) = sprint {
        add_to_sprint(ctx, &response.key, sprint_id).await?;
    }

    tracing::info!(key = %response.key, id = %response.id, "Issue created successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Created issue: {}", response.key),
        &MutationResult::with_id(format!("Created issue: {}", response.key), &response.key),
    )
}

/// Build the PUT body for `PUT /rest/api/3/issue/{key}`. Pure, testable.
pub(crate) fn build_update_payload(
    summary: Option<&str>,
    description: Option<&str>,
    priority: Option<&str>,
    custom: &HashMap<String, Value>,
) -> Result<Value> {
    use serde_json::json;

    check_field_collisions(
        custom,
        &[],
        &[
            ("summary", "--summary", summary.is_some()),
            ("description", "--description", description.is_some()),
            ("priority", "--priority", priority.is_some()),
        ],
    )?;

    let mut fields = json!({});
    if let Some(s) = summary {
        fields["summary"] = json!(s);
    }
    if let Some(desc) = description {
        fields["description"] = markdown_to_adf(desc);
    }
    if let Some(pri) = priority {
        fields["priority"] = json!({ "name": pri });
    }
    for (key, value) in custom {
        fields[key] = value.clone();
    }

    Ok(json!({ "fields": fields }))
}

/// Build the body for `POST /rest/agile/1.0/sprint/{id}/issue`. Pure, testable.
fn build_sprint_add_payload(key: &str) -> Value {
    serde_json::json!({ "issues": [key] })
}

/// Add an issue to a sprint via the Agile API. This is the reliable way to set a
/// sprint (no per-instance customfield id, and it avoids the "Number value expected"
/// quirk of writing the sprint field directly).
async fn add_to_sprint(ctx: &JiraContext<'_>, key: &str, sprint_id: &str) -> Result<()> {
    let id: u64 = sprint_id.parse().map_err(|_| {
        anyhow!("Invalid --sprint '{sprint_id}': expected a numeric sprint id (e.g. 25446)")
    })?;
    let payload = build_sprint_add_payload(key);
    let _: Value = ctx
        .client
        .post(&format!("/rest/agile/1.0/sprint/{id}/issue"), &payload)
        .await
        .with_context(|| format!("Failed to add {key} to sprint {id}"))?;
    Ok(())
}

/// Summarize an issue's Jira sprint field (customfield_10020) for display, e.g.
/// "Sprint 12 (active)". Prefers an active sprint, else the most recent entry.
fn extract_active_sprint(value: &Value) -> Option<String> {
    let arr = value.as_array()?;
    let pick = arr
        .iter()
        .find(|s| s.get("state").and_then(Value::as_str) == Some("active"))
        .or_else(|| arr.last())?;
    let name = pick.get("name").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() {
        return None;
    }
    match pick.get("state").and_then(Value::as_str) {
        Some(state) if !state.is_empty() => Some(format!("{name} ({state})")),
        _ => Some(name.to_string()),
    }
}

pub async fn update_issue(
    ctx: &JiraContext<'_>,
    key: &str,
    summary: Option<&str>,
    description: Option<&str>,
    priority: Option<&str>,
    custom_fields: &HashMap<String, Value>,
    sprint: Option<&str>,
) -> Result<()> {
    let has_fields = summary.is_some()
        || description.is_some()
        || priority.is_some()
        || !custom_fields.is_empty();
    if !has_fields && sprint.is_none() {
        bail!(
            "Nothing to update for {key}: provide --summary, --description, --priority, --field, or --sprint"
        );
    }

    if has_fields {
        let payload = build_update_payload(summary, description, priority, custom_fields)?;
        let _: Value = ctx
            .client
            .put(&format!("/rest/api/3/issue/{key}"), &payload)
            .await
            .with_context(|| format!("Failed to update issue {key}"))?;
    }

    if let Some(sprint_id) = sprint {
        add_to_sprint(ctx, key, sprint_id).await?;
    }

    tracing::info!(%key, "Issue updated successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Updated issue: {key}"),
        &MutationResult::with_id(format!("Updated issue: {key}"), key),
    )
}

pub async fn delete_issue(ctx: &JiraContext<'_>, key: &str, force: bool) -> Result<()> {
    if !force {
        println!(
            "⚠️  This will permanently delete issue {}. Use --force to confirm.",
            key
        );
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/issue/{key}"))
        .await
        .with_context(|| format!("Failed to delete issue {key}"))?;

    tracing::info!(%key, "Issue deleted successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Deleted issue: {key}"),
        &MutationResult::with_id(format!("Deleted issue: {key}"), key),
    )
}

#[derive(Deserialize)]
struct TransitionsResponse {
    transitions: Vec<Transition>,
}

#[derive(Deserialize)]
struct Transition {
    id: String,
    name: String,
    /// The status the issue lands in. Absent on some older instances.
    #[serde(default)]
    to: Option<StatusField>,
}

async fn fetch_transitions(ctx: &JiraContext<'_>, key: &str) -> Result<Vec<Transition>> {
    let available: TransitionsResponse = ctx
        .client
        .get(&format!("/rest/api/3/issue/{key}/transitions"))
        .await
        .with_context(|| format!("Failed to get transitions for {key}"))?;
    Ok(available.transitions)
}

/// List the transitions available on an issue right now.
///
/// Which transitions exist depends on the workflow and the issue's current
/// status, so the only reliable source is the issue itself. Without this,
/// `jira issue transition` had to be driven by guesswork (#101).
pub async fn list_transitions(ctx: &JiraContext<'_>, key: &str) -> Result<()> {
    let transitions = fetch_transitions(ctx, key).await?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        to: &'a str,
    }

    let rows: Vec<Row<'_>> = transitions
        .iter()
        .map(|t| Row {
            id: t.id.as_str(),
            name: t.name.as_str(),
            to: t.to.as_ref().map(|s| s.name.as_str()).unwrap_or(""),
        })
        .collect();

    ctx.renderer.render_list(&rows)
}

pub async fn transition_issue(ctx: &JiraContext<'_>, key: &str, transition: &str) -> Result<()> {
    use serde_json::json;

    let available = fetch_transitions(ctx, key).await?;

    let target = available
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(transition) || t.id == transition)
        .ok_or_else(|| anyhow::anyhow!("Transition '{}' not found", transition))?;

    let payload = json!({ "transition": { "id": target.id } });

    let _: Value = ctx
        .client
        .post(&format!("/rest/api/3/issue/{key}/transitions"), &payload)
        .await
        .with_context(|| format!("Failed to transition issue {key}"))?;

    tracing::info!(%key, transition = %target.name, "Issue transitioned successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Transitioned {key} to: {}", target.name),
        &MutationResult::with_id(format!("Transitioned to: {}", target.name), key),
    )
}

pub async fn assign_issue(ctx: &JiraContext<'_>, key: &str, assignee: &str) -> Result<()> {
    use serde_json::json;

    let payload = json!({ "accountId": assignee });

    let _: Value = ctx
        .client
        .put(&format!("/rest/api/3/issue/{key}/assignee"), &payload)
        .await
        .with_context(|| format!("Failed to assign issue {key}"))?;

    tracing::info!(%key, %assignee, "Issue assigned successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Assigned {key} to: {assignee}"),
        &MutationResult::with_id(format!("Assigned to: {assignee}"), key),
    )
}

pub async fn unassign_issue(ctx: &JiraContext<'_>, key: &str) -> Result<()> {
    use serde_json::json;

    let payload = json!({ "accountId": null });

    let _: Value = ctx
        .client
        .put(&format!("/rest/api/3/issue/{key}/assignee"), &payload)
        .await
        .with_context(|| format!("Failed to unassign issue {key}"))?;

    tracing::info!(%key, "Issue unassigned successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Unassigned: {key}"),
        &MutationResult::with_id(format!("Unassigned: {key}"), key),
    )
}

// Watcher operations

pub async fn list_watchers(ctx: &JiraContext<'_>, key: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct WatchersResponse {
        watchers: Vec<Watcher>,
    }

    #[derive(Deserialize)]
    struct Watcher {
        #[serde(rename = "accountId")]
        account_id: String,
        #[serde(rename = "displayName")]
        display_name: String,
        #[serde(rename = "emailAddress", default)]
        email: Option<String>,
    }

    let response: WatchersResponse = ctx
        .client
        .get(&format!("/rest/api/3/issue/{key}/watchers"))
        .await
        .with_context(|| format!("Failed to get watchers for {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        display_name: &'a str,
        email: &'a str,
        account_id: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .watchers
        .iter()
        .map(|w| Row {
            display_name: w.display_name.as_str(),
            email: w.email.as_deref().unwrap_or(""),
            account_id: w.account_id.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

pub async fn add_watcher(ctx: &JiraContext<'_>, key: &str, user: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .post(
            &format!("/rest/api/3/issue/{key}/watchers"),
            &user.to_string(),
        )
        .await
        .with_context(|| format!("Failed to add watcher to {key}"))?;

    tracing::info!(%key, %user, "Watcher added successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Added watcher to {key}: {user}"),
        &MutationResult::with_id(format!("Added watcher: {user}"), key),
    )
}

pub async fn remove_watcher(ctx: &JiraContext<'_>, key: &str, user: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!(
            "/rest/api/3/issue/{key}/watchers?accountId={user}"
        ))
        .await
        .with_context(|| format!("Failed to remove watcher from {key}"))?;

    tracing::info!(%key, %user, "Watcher removed successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Removed watcher from {key}: {user}"),
        &MutationResult::with_id(format!("Removed watcher: {user}"), key),
    )
}

// Link operations

pub async fn list_links(ctx: &JiraContext<'_>, key: &str) -> Result<()> {
    let _issue: Issue = ctx
        .client
        .get(&format!("/rest/api/3/issue/{key}?fields=issuelinks"))
        .await
        .with_context(|| format!("Failed to get issue {key}"))?;

    // Note: This is simplified - real implementation would need proper IssueLink deserialization
    tracing::info!(%key, "Links listed successfully");
    println!("Links for {}: (full implementation pending)", key);
    Ok(())
}

pub async fn create_link(
    ctx: &JiraContext<'_>,
    from: &str,
    to: &str,
    link_type: &str,
) -> Result<()> {
    use serde_json::json;

    let payload = json!({
        "type": { "name": link_type },
        "inwardIssue": { "key": from },
        "outwardIssue": { "key": to },
    });

    let _: Value = ctx
        .client
        .post("/rest/api/3/issueLink", &payload)
        .await
        .context("Failed to create issue link")?;

    tracing::info!(%from, %to, %link_type, "Issue link created successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Linked {from} to {to} ({link_type})"),
        &MutationResult::new(format!("Linked {from} to {to} ({link_type})")),
    )
}

pub async fn delete_link(ctx: &JiraContext<'_>, link_id: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/issueLink/{link_id}"))
        .await
        .with_context(|| format!("Failed to delete link {link_id}"))?;

    tracing::info!(%link_id, "Issue link deleted successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Deleted link: {link_id}"),
        &MutationResult::with_id(format!("Deleted link: {link_id}"), link_id),
    )
}

// Comment operations

pub async fn list_comments(ctx: &JiraContext<'_>, key: &str, full: bool) -> Result<()> {
    #[derive(Deserialize)]
    struct CommentsResponse {
        comments: Vec<Comment>,
    }

    #[derive(Deserialize)]
    struct Comment {
        id: String,
        body: Value,
        author: UserField,
        created: String,
    }

    #[derive(Deserialize)]
    struct UserField {
        #[serde(rename = "displayName")]
        display_name: String,
    }

    let response: CommentsResponse = ctx
        .client
        .get(&format!("/rest/api/3/issue/{key}/comment"))
        .await
        .with_context(|| format!("Failed to get comments for {key}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        author: &'a str,
        created: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        body_preview: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    }

    let rows: Vec<Row<'_>> = response
        .comments
        .iter()
        .map(|c| {
            let text = extract_adf_text(&c.body);
            let (body_preview, body) = if full {
                (None, Some(text))
            } else {
                (Some(text.chars().take(50).collect::<String>()), None)
            };
            Row {
                id: c.id.as_str(),
                author: c.author.display_name.as_str(),
                created: c.created.as_str(),
                body_preview,
                body,
            }
        })
        .collect();

    ctx.renderer.render(&rows)
}

pub async fn add_comment(ctx: &JiraContext<'_>, key: &str, body: &str) -> Result<()> {
    use serde_json::json;

    let payload = json!({ "body": markdown_to_adf(body) });

    let _: Value = ctx
        .client
        .post(&format!("/rest/api/3/issue/{key}/comment"), &payload)
        .await
        .with_context(|| format!("Failed to add comment to {key}"))?;

    tracing::info!(%key, "Comment added successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Added comment to: {key}"),
        &MutationResult::with_id(format!("Added comment to: {key}"), key),
    )
}

/// Update a comment.
///
/// The issue key is required because Jira Cloud has no top-level comment route:
/// comments live under their issue at
/// `/rest/api/3/issue/{issueIdOrKey}/comment/{id}`. This previously PUT to
/// `/rest/api/3/comment/{id}`, which 404s on every call (#100).
pub async fn update_comment(
    ctx: &JiraContext<'_>,
    key: &str,
    comment_id: &str,
    body: &str,
) -> Result<()> {
    use serde_json::json;

    let payload = json!({ "body": markdown_to_adf(body) });

    let _: Value = ctx
        .client
        .put(
            &format!("/rest/api/3/issue/{key}/comment/{comment_id}"),
            &payload,
        )
        .await
        .with_context(|| format!("Failed to update comment {comment_id}"))?;

    tracing::info!(%comment_id, "Comment updated successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Updated comment: {comment_id}"),
        &MutationResult::with_id(format!("Updated comment: {comment_id}"), comment_id),
    )
}

/// Delete a comment. Same routing fix as `update_comment` (#100).
pub async fn delete_comment(ctx: &JiraContext<'_>, key: &str, comment_id: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/issue/{key}/comment/{comment_id}"))
        .await
        .with_context(|| format!("Failed to delete comment {comment_id}"))?;

    tracing::info!(%comment_id, "Comment deleted successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Deleted comment: {comment_id}"),
        &MutationResult::with_id(format!("Deleted comment: {comment_id}"), comment_id),
    )
}

// Issue-related data structures

#[derive(Deserialize)]
struct Issue {
    key: String,
    fields: IssueFields,
}

#[derive(Deserialize)]
struct IssueFields {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    status: Option<StatusField>,
    #[serde(default)]
    assignee: Option<UserField>,
    #[serde(default)]
    reporter: Option<UserField>,
    #[serde(default)]
    description: Option<Value>,
    #[serde(default)]
    issuetype: Option<IssueTypeField>,
    #[serde(default)]
    attachment: Vec<JiraAttachment>,
    // customfield_10020 is the standard Jira Cloud "Sprint" field (an array of
    // sprint objects). Optional so instances that omit/remap it never break the parse.
    #[serde(rename = "customfield_10020", default)]
    sprint: Option<Value>,
}

#[derive(Deserialize)]
struct StatusField {
    name: String,
}

#[derive(Deserialize)]
struct UserField {
    #[serde(rename = "displayName")]
    display_name: String,
}

#[derive(Deserialize)]
struct IssueTypeField {
    name: String,
}

// ADF (Atlassian Document Format) text extraction

const ADF_MAX_DEPTH: usize = 64;

/// Recursively extract plain text from an ADF (Atlassian Document Format) JSON value.
/// Handles paragraphs, headings, lists, code blocks, hard breaks, inline nodes, and nested content.
fn extract_adf_text(value: &Value) -> String {
    extract_adf_inner(value, 0)
}

fn extract_adf_inner(value: &Value, depth: usize) -> String {
    if depth > ADF_MAX_DEPTH {
        return String::new();
    }

    // Plain string fallback (defensive — v3 API always returns ADF objects)
    if let Some(s) = value.as_str() {
        return s.to_string();
    }

    // Null or non-object
    if !value.is_object() {
        return String::new();
    }

    let node_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let attrs = value.get("attrs");

    match node_type {
        "text" => value
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        "hardBreak" => "\n".to_string(),
        // Inline nodes — extract from attrs
        "mention" => attrs
            .and_then(|a| a.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        "emoji" => attrs
            .and_then(|a| a.get("shortName"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        "inlineCard" => attrs
            .and_then(|a| a.get("url"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        "status" => attrs
            .and_then(|a| a.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        // List types — handle numbering at list level
        "bulletList" => {
            let items = value
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|item| {
                            let text = extract_list_item_text(item, depth + 1);
                            format!("- {text}")
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            items.join("\n")
        }
        "orderedList" => {
            let items = value
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let text = extract_list_item_text(item, depth + 1);
                            format!("{}. {text}", i + 1)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            items.join("\n")
        }
        "listItem" => {
            // Fallback if listItem is encountered outside list context
            let text = extract_list_item_text(value, depth);
            format!("- {text}")
        }
        _ => {
            // Recurse into content array
            let children = value
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|child| extract_adf_inner(child, depth + 1))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            match node_type {
                "paragraph" | "heading" | "codeBlock" => children.join(""),
                // doc, or any unknown wrapper — join blocks with newlines
                _ => children
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            }
        }
    }
}

/// Extract text from a listItem node, joining multiple child blocks with newlines.
fn extract_list_item_text(item: &Value, depth: usize) -> String {
    item.get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|child| extract_adf_inner(child, depth + 1))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

// ADF to Markdown conversion

/// Recursively convert an ADF JSON value to Markdown.
fn extract_adf_markdown(value: &Value) -> String {
    extract_adf_md_inner(value, 0)
}

fn extract_adf_md_inner(value: &Value, depth: usize) -> String {
    if depth > ADF_MAX_DEPTH {
        return String::new();
    }

    if let Some(s) = value.as_str() {
        return s.to_string();
    }

    if !value.is_object() {
        return String::new();
    }

    let node_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let attrs = value.get("attrs");

    match node_type {
        "text" => {
            let raw = value.get("text").and_then(|t| t.as_str()).unwrap_or("");
            let marks = value.get("marks");
            apply_marks(raw, marks)
        }
        "hardBreak" => "\n".to_string(),
        "rule" => "---".to_string(),
        // Inline nodes
        "mention" => {
            let name = attrs
                .and_then(|a| a.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            format!("@{}", name.trim_start_matches('@'))
        }
        "emoji" => attrs
            .and_then(|a| a.get("shortName"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        "inlineCard" => {
            let url = attrs
                .and_then(|a| a.get("url"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            format!("[{url}]({url})")
        }
        "status" => {
            let text = attrs
                .and_then(|a| a.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            format!("`{text}`")
        }
        "heading" => {
            let level = attrs
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1) as usize;
            let prefix = "#".repeat(level.min(6));
            let children = collect_md_children(value, depth);
            format!("{prefix} {children}")
        }
        "paragraph" => collect_md_children(value, depth),
        "codeBlock" => {
            let lang = attrs
                .and_then(|a| a.get("language"))
                .and_then(|l| l.as_str())
                .unwrap_or("");
            let code = collect_md_children(value, depth);
            format!("```{lang}\n{code}\n```")
        }
        "blockquote" => {
            let inner = collect_md_block_children(value, depth);
            inner
                .lines()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        }
        "bulletList" => {
            let items = value
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|item| {
                            let text = extract_md_list_item(item, depth + 1);
                            format!("- {text}")
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            items.join("\n")
        }
        "orderedList" => {
            let items = value
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let text = extract_md_list_item(item, depth + 1);
                            format!("{}. {text}", i + 1)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            items.join("\n")
        }
        "listItem" => {
            let text = extract_md_list_item(value, depth);
            format!("- {text}")
        }
        // doc or unknown wrapper
        _ => collect_md_block_children(value, depth),
    }
}

/// Collect inline children (text, marks, etc.) into a single string.
fn collect_md_children(value: &Value, depth: usize) -> String {
    value
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|child| extract_adf_md_inner(child, depth + 1))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Collect block-level children separated by double newlines.
fn collect_md_block_children(value: &Value, depth: usize) -> String {
    value
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|child| extract_adf_md_inner(child, depth + 1))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default()
}

/// Extract markdown text from a listItem node.
fn extract_md_list_item(item: &Value, depth: usize) -> String {
    item.get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|child| extract_adf_md_inner(child, depth + 1))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Apply ADF text marks to produce markdown formatting.
fn apply_marks(text: &str, marks: Option<&Value>) -> String {
    let marks_arr = match marks.and_then(|m| m.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return text.to_string(),
    };

    let mut result = text.to_string();

    for mark in marks_arr {
        let mark_type = mark.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match mark_type {
            "strong" => result = format!("**{result}**"),
            "em" => result = format!("*{result}*"),
            "code" => result = format!("`{result}`"),
            "strike" => result = format!("~~{result}~~"),
            "underline" => result = format!("_{result}_"),
            "link" => {
                let href = mark
                    .get("attrs")
                    .and_then(|a| a.get("href"))
                    .and_then(|h| h.as_str())
                    .unwrap_or("");
                result = format!("[{result}]({href})");
            }
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_adf_text_simple() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{"type": "text", "text": "hello world"}]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "hello world");
    }

    #[test]
    fn test_extract_adf_text_multiple_paragraphs() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "first"}]
                },
                {
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "second"}]
                }
            ]
        });
        assert_eq!(extract_adf_text(&adf), "first\nsecond");
    }

    #[test]
    fn test_extract_adf_text_heading_and_paragraph() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "heading",
                    "attrs": {"level": 1},
                    "content": [{"type": "text", "text": "Title"}]
                },
                {
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "Body text"}]
                }
            ]
        });
        assert_eq!(extract_adf_text(&adf), "Title\nBody text");
    }

    #[test]
    fn test_extract_adf_text_bullet_list() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "bulletList",
                "content": [
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "item one"}]
                        }]
                    },
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "item two"}]
                        }]
                    }
                ]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "- item one\n- item two");
    }

    #[test]
    fn test_extract_adf_text_code_block() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "codeBlock",
                "attrs": {"language": "rust"},
                "content": [{"type": "text", "text": "fn main() {}"}]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "fn main() {}");
    }

    #[test]
    fn test_extract_adf_text_hard_break() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "line one"},
                    {"type": "hardBreak"},
                    {"type": "text", "text": "line two"}
                ]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "line one\nline two");
    }

    #[test]
    fn test_extract_adf_text_null() {
        assert_eq!(extract_adf_text(&Value::Null), "");
    }

    #[test]
    fn test_extract_adf_text_plain_string() {
        let val = json!("plain string fallback");
        assert_eq!(extract_adf_text(&val), "plain string fallback");
    }

    #[test]
    fn test_issue_fields_deserialize_adf_description() {
        let json_str = r#"{
            "summary": "Test issue",
            "description": {
                "type": "doc",
                "version": 1,
                "content": [{
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "ADF description"}]
                }]
            }
        }"#;
        let fields: IssueFields = serde_json::from_str(json_str).unwrap();
        assert_eq!(fields.summary.as_deref(), Some("Test issue"));
        let desc = fields.description.as_ref().map(extract_adf_text).unwrap();
        assert_eq!(desc, "ADF description");
    }

    #[test]
    fn test_issue_fields_deserialize_null_description() {
        let json_str = r#"{
            "summary": "No desc"
        }"#;
        let fields: IssueFields = serde_json::from_str(json_str).unwrap();
        assert!(fields.description.is_none());
    }

    #[test]
    fn test_issue_fields_deserialize_attachments_numeric_id() {
        // Jira commonly returns attachment `id` as a number; this must not abort
        // the whole issue parse (regression guard for #63 / PR #64).
        let json_str = r#"{
            "summary": "Has attachments",
            "attachment": [
                {"id": 10000, "filename": "diagram.png", "mimeType": "image/png", "size": 98150, "content": "https://x/secure/attachment/10000/diagram.png"},
                {"id": "10001", "filename": "notes.txt", "mimeType": "text/plain", "size": 12, "content": "https://x/2"}
            ]
        }"#;
        let fields: IssueFields = serde_json::from_str(json_str).unwrap();
        assert_eq!(fields.attachment.len(), 2);
        assert_eq!(fields.attachment[0].id.as_deref(), Some("10000"));
        assert_eq!(
            fields.attachment[0].filename.as_deref(),
            Some("diagram.png")
        );
        assert_eq!(fields.attachment[0].mime_type.as_deref(), Some("image/png"));
        assert_eq!(fields.attachment[0].size, Some(98150));
        // string id also accepted
        assert_eq!(fields.attachment[1].id.as_deref(), Some("10001"));
    }

    #[test]
    fn test_issue_fields_deserialize_attachment_partial_fields() {
        // A missing/null nested property must not fail the whole parse.
        let json_str = r#"{
            "attachment": [{"id": 5, "filename": null}]
        }"#;
        let fields: IssueFields = serde_json::from_str(json_str).unwrap();
        assert_eq!(fields.attachment.len(), 1);
        assert_eq!(fields.attachment[0].id.as_deref(), Some("5"));
        assert!(fields.attachment[0].filename.is_none());
        assert!(fields.attachment[0].content.is_none());
    }

    #[test]
    fn test_issue_fields_no_attachment_field() {
        let fields: IssueFields = serde_json::from_str(r#"{"summary": "x"}"#).unwrap();
        assert!(fields.attachment.is_empty());
    }

    #[test]
    fn test_build_sprint_add_payload() {
        let p = build_sprint_add_payload("DEV-1");
        assert_eq!(p, serde_json::json!({ "issues": ["DEV-1"] }));
    }

    #[test]
    fn test_extract_active_sprint_prefers_active() {
        // Issue is in a closed and an active sprint; show the active one.
        let v = serde_json::json!([
            {"id": 1, "name": "Sprint 11", "state": "closed"},
            {"id": 2, "name": "Sprint 12", "state": "active"}
        ]);
        assert_eq!(
            extract_active_sprint(&v).as_deref(),
            Some("Sprint 12 (active)")
        );
    }

    #[test]
    fn test_extract_active_sprint_falls_back_to_last() {
        let v = serde_json::json!([{"id": 1, "name": "Sprint 9", "state": "closed"}]);
        assert_eq!(
            extract_active_sprint(&v).as_deref(),
            Some("Sprint 9 (closed)")
        );
        // empty / non-array -> None
        assert_eq!(extract_active_sprint(&serde_json::json!([])), None);
        assert_eq!(extract_active_sprint(&serde_json::json!("x")), None);
    }

    #[test]
    fn test_issue_fields_deserialize_sprint() {
        let json = r#"{
            "summary": "S",
            "customfield_10020": [{"id": 25446, "name": "Sprint 12", "state": "active"}]
        }"#;
        let fields: IssueFields = serde_json::from_str(json).unwrap();
        let sprint = fields.sprint.as_ref().and_then(extract_active_sprint);
        assert_eq!(sprint.as_deref(), Some("Sprint 12 (active)"));
    }

    #[test]
    fn test_issue_fields_sprint_absent_is_none() {
        let fields: IssueFields = serde_json::from_str(r#"{"summary": "x"}"#).unwrap();
        assert!(fields.sprint.is_none());
    }

    #[test]
    fn test_attachments_markdown_empty_and_populated() {
        assert_eq!(attachments_markdown(&[]), "");
        let json_str = r#"{"attachment":[{"id":7,"filename":"a.png","mimeType":"image/png","size":10,"content":"https://u"}]}"#;
        let fields: IssueFields = serde_json::from_str(json_str).unwrap();
        let md = attachments_markdown(&fields.attachment);
        assert!(md.contains("## Attachments"));
        assert!(md.contains("a.png"));
        assert!(md.contains("https://u"));
    }

    #[test]
    fn test_extract_adf_text_ordered_list() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "orderedList",
                "content": [
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "first"}]
                        }]
                    },
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "second"}]
                        }]
                    },
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "third"}]
                        }]
                    }
                ]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "1. first\n2. second\n3. third");
    }

    #[test]
    fn test_extract_adf_text_nested_list_item() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "bulletList",
                "content": [{
                    "type": "listItem",
                    "content": [
                        {
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "paragraph one"}]
                        },
                        {
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "paragraph two"}]
                        }
                    ]
                }]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "- paragraph one\nparagraph two");
    }

    #[test]
    fn test_extract_adf_text_mention() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "assigned to "},
                    {"type": "mention", "attrs": {"id": "123", "text": "@John Doe"}}
                ]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "assigned to @John Doe");
    }

    #[test]
    fn test_extract_adf_text_emoji() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "great work "},
                    {"type": "emoji", "attrs": {"shortName": ":thumbsup:"}}
                ]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "great work :thumbsup:");
    }

    #[test]
    fn test_extract_adf_text_inline_card() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "see "},
                    {"type": "inlineCard", "attrs": {"url": "https://example.com/page"}}
                ]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "see https://example.com/page");
    }

    #[test]
    fn test_extract_adf_text_status() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "status: "},
                    {"type": "status", "attrs": {"text": "IN PROGRESS", "color": "blue"}}
                ]
            }]
        });
        assert_eq!(extract_adf_text(&adf), "status: IN PROGRESS");
    }

    #[test]
    fn test_extract_adf_text_depth_guard() {
        // Build deeply nested ADF that exceeds MAX_DEPTH
        let mut node = json!({"type": "text", "text": "deep"});
        for _ in 0..70 {
            node = json!({
                "type": "paragraph",
                "content": [node]
            });
        }
        let adf = json!({"type": "doc", "version": 1, "content": [node]});
        // Should not panic — depth guard returns empty string
        let result = extract_adf_text(&adf);
        assert!(result.is_empty() || result == "deep");
    }

    // -- Markdown extraction tests --

    #[test]
    fn test_extract_adf_markdown_heading() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "heading",
                "attrs": {"level": 2},
                "content": [{"type": "text", "text": "Section Title"}]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "## Section Title");
    }

    #[test]
    fn test_extract_adf_markdown_bold() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{"type": "text", "text": "important", "marks": [{"type": "strong"}]}]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "**important**");
    }

    #[test]
    fn test_extract_adf_markdown_italic() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{"type": "text", "text": "emphasis", "marks": [{"type": "em"}]}]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "*emphasis*");
    }

    #[test]
    fn test_extract_adf_markdown_link() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "click here",
                    "marks": [{"type": "link", "attrs": {"href": "https://example.com"}}]
                }]
            }]
        });
        assert_eq!(
            extract_adf_markdown(&adf),
            "[click here](https://example.com)"
        );
    }

    #[test]
    fn test_extract_adf_markdown_code_block() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "codeBlock",
                "attrs": {"language": "rust"},
                "content": [{"type": "text", "text": "fn main() {}"}]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "```rust\nfn main() {}\n```");
    }

    #[test]
    fn test_extract_adf_markdown_bullet_list() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "bulletList",
                "content": [
                    {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "one"}]}]},
                    {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "two"}]}]}
                ]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "- one\n- two");
    }

    #[test]
    fn test_extract_adf_markdown_ordered_list() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "orderedList",
                "content": [
                    {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "first"}]}]},
                    {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "second"}]}]}
                ]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "1. first\n2. second");
    }

    #[test]
    fn test_extract_adf_markdown_inline_code() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{"type": "text", "text": "var_name", "marks": [{"type": "code"}]}]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "`var_name`");
    }

    #[test]
    fn test_extract_adf_markdown_blockquote() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "blockquote",
                "content": [{
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "quoted text"}]
                }]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "> quoted text");
    }

    #[test]
    fn test_extract_adf_markdown_multiple_paragraphs() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {"type": "paragraph", "content": [{"type": "text", "text": "first"}]},
                {"type": "paragraph", "content": [{"type": "text", "text": "second"}]}
            ]
        });
        assert_eq!(extract_adf_markdown(&adf), "first\n\nsecond");
    }

    #[test]
    fn test_extract_adf_markdown_mixed_marks() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "bold link",
                    "marks": [
                        {"type": "strong"},
                        {"type": "link", "attrs": {"href": "https://example.com"}}
                    ]
                }]
            }]
        });
        assert_eq!(
            extract_adf_markdown(&adf),
            "[**bold link**](https://example.com)"
        );
    }

    #[test]
    fn test_extract_adf_markdown_status_node() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "status: "},
                    {"type": "status", "attrs": {"text": "IN PROGRESS", "color": "blue"}}
                ]
            }]
        });
        assert_eq!(extract_adf_markdown(&adf), "status: `IN PROGRESS`");
    }

    // -- Custom field parsing tests --

    #[test]
    fn test_parse_custom_fields_single() {
        let input = vec![r#"customfield_10001={"value":"Alpha"}"#.to_string()];
        let result = parse_custom_fields(&input).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result["customfield_10001"], json!({"value": "Alpha"}));
    }

    #[test]
    fn test_parse_custom_fields_multiple() {
        let input = vec![
            r#"customfield_10001={"value":"Alpha"}"#.to_string(),
            r#"customfield_10002=[{"value":"Beta"}]"#.to_string(),
        ];
        let result = parse_custom_fields(&input).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result["customfield_10001"], json!({"value": "Alpha"}));
        assert_eq!(result["customfield_10002"], json!([{"value": "Beta"}]));
    }

    #[test]
    fn test_parse_custom_fields_empty() {
        let result = parse_custom_fields(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_custom_fields_string_value() {
        let input = vec![r#"customfield_10001="just a string""#.to_string()];
        let result = parse_custom_fields(&input).unwrap();
        assert_eq!(result["customfield_10001"], json!("just a string"));
    }

    #[test]
    fn test_parse_custom_fields_numeric_value() {
        let input = vec!["customfield_10001=42".to_string()];
        let result = parse_custom_fields(&input).unwrap();
        assert_eq!(result["customfield_10001"], json!(42));
    }

    #[test]
    fn test_parse_custom_fields_missing_equals() {
        let input = vec!["customfield_10001_no_value".to_string()];
        let result = parse_custom_fields(&input);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected key=JSON_VALUE"));
    }

    #[test]
    fn test_parse_custom_fields_invalid_json() {
        let input = vec!["customfield_10001={not valid json}".to_string()];
        let result = parse_custom_fields(&input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_parse_custom_fields_equals_in_json_value() {
        let input = vec![r#"customfield_10001={"formula":"a=b"}"#.to_string()];
        let result = parse_custom_fields(&input).unwrap();
        assert_eq!(result["customfield_10001"], json!({"formula": "a=b"}));
    }

    #[test]
    fn test_parse_custom_fields_duplicate_key_rejected() {
        let input = vec![
            r#"customfield_10001={"value":"first"}"#.to_string(),
            r#"customfield_10001={"value":"second"}"#.to_string(),
        ];
        let err = parse_custom_fields(&input).unwrap_err().to_string();
        assert!(err.contains("more than once"), "got: {err}");
        assert!(err.contains("customfield_10001"), "got: {err}");
    }

    // -- Payload assembly + collision tests --

    fn cf(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn build_create_payload_merges_typed_and_custom() {
        let custom = cf(&[("customfield_10010", json!({"value": "x"}))]);
        let payload = build_create_payload(
            "PROJ",
            "Task",
            "hello",
            Some("desc"),
            Some("u1"),
            Some("High"),
            &custom,
        )
        .unwrap();
        let fields = &payload["fields"];
        assert_eq!(fields["project"]["key"], "PROJ");
        assert_eq!(fields["issuetype"]["name"], "Task");
        assert_eq!(fields["summary"], "hello");
        assert_eq!(fields["assignee"]["id"], "u1");
        assert_eq!(fields["priority"]["name"], "High");
        assert_eq!(fields["customfield_10010"], json!({"value": "x"}));
        // description wrapped as ADF
        assert_eq!(fields["description"]["type"], "doc");
    }

    #[test]
    fn build_create_payload_rejects_mandatory_collision() {
        let custom = cf(&[("summary", json!("x"))]);
        let err = build_create_payload("PROJ", "Task", "s", None, None, None, &custom)
            .unwrap_err()
            .to_string();
        assert!(err.contains("reserved"), "got: {err}");
        assert!(err.contains("--summary"), "got: {err}");
    }

    #[test]
    fn build_create_payload_rejects_optional_when_both_set() {
        let custom = cf(&[("description", json!({"type": "doc"}))]);
        let err =
            build_create_payload("PROJ", "Task", "s", Some("typed desc"), None, None, &custom)
                .unwrap_err()
                .to_string();
        assert!(err.contains("collides"), "got: {err}");
        assert!(err.contains("--description"), "got: {err}");
    }

    #[test]
    fn build_create_payload_allows_raw_only_optional() {
        let raw_desc = json!({"type": "doc", "version": 1, "content": []});
        let custom = cf(&[("description", raw_desc.clone())]);
        let payload = build_create_payload("PROJ", "Task", "s", None, None, None, &custom).unwrap();
        assert_eq!(payload["fields"]["description"], raw_desc);
    }

    #[test]
    fn build_update_payload_rejects_summary_collision() {
        let custom = cf(&[("summary", json!("raw"))]);
        let err = build_update_payload(Some("typed"), None, None, &custom)
            .unwrap_err()
            .to_string();
        assert!(err.contains("collides"), "got: {err}");
        assert!(err.contains("--summary"), "got: {err}");
    }

    #[test]
    fn build_update_payload_raw_only_priority_ok() {
        let custom = cf(&[("priority", json!({"name": "Low"}))]);
        let payload = build_update_payload(None, None, None, &custom).unwrap();
        assert_eq!(payload["fields"]["priority"], json!({"name": "Low"}));
    }
}
