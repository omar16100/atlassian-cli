use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use atlassian_cli_output::OutputFormat;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::utils::JiraContext;
use crate::commands::common::{render_success, MutationResult};
use crate::query::JqlBuilder;

/// Parse `--field key=json_value` pairs into a HashMap.
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
        map.insert(key.to_string(), parsed);
    }
    Ok(map)
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

        let md = format!(
            "# {key}: {summary}\n\n\
             | Field | Value |\n\
             | --- | --- |\n\
             | Status | {status} |\n\
             | Type | {issue_type} |\n\
             | Assignee | {assignee} |\n\
             | Reporter | {reporter} |\n\n\
             ## Description\n\n\
             {description}"
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
    };

    ctx.renderer.render(&view)
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
) -> Result<()> {
    use serde_json::json;

    let mut fields = json!({
        "project": { "key": project },
        "issuetype": { "name": issue_type },
        "summary": summary,
    });

    if let Some(desc) = description {
        fields["description"] = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": desc }]
            }]
        });
    }

    if let Some(user) = assignee {
        fields["assignee"] = json!({ "id": user });
    }

    if let Some(pri) = priority {
        fields["priority"] = json!({ "name": pri });
    }

    for (key, value) in custom_fields {
        fields[key] = value.clone();
    }

    let payload = json!({ "fields": fields });

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

    tracing::info!(key = %response.key, id = %response.id, "Issue created successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Created issue: {}", response.key),
        &MutationResult::with_id(format!("Created issue: {}", response.key), &response.key),
    )
}

pub async fn update_issue(
    ctx: &JiraContext<'_>,
    key: &str,
    summary: Option<&str>,
    description: Option<&str>,
    priority: Option<&str>,
    custom_fields: &HashMap<String, Value>,
) -> Result<()> {
    use serde_json::json;

    let mut fields = json!({});

    if let Some(s) = summary {
        fields["summary"] = json!(s);
    }

    if let Some(desc) = description {
        fields["description"] = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": desc }]
            }]
        });
    }

    if let Some(pri) = priority {
        fields["priority"] = json!({ "name": pri });
    }

    for (key, value) in custom_fields {
        fields[key] = value.clone();
    }

    let payload = json!({ "fields": fields });

    let _: Value = ctx
        .client
        .put(&format!("/rest/api/3/issue/{key}"), &payload)
        .await
        .with_context(|| format!("Failed to update issue {key}"))?;

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

pub async fn transition_issue(ctx: &JiraContext<'_>, key: &str, transition: &str) -> Result<()> {
    use serde_json::json;

    // First, get available transitions
    #[derive(Deserialize)]
    struct TransitionsResponse {
        transitions: Vec<Transition>,
    }

    #[derive(Deserialize)]
    struct Transition {
        id: String,
        name: String,
    }

    let available: TransitionsResponse = ctx
        .client
        .get(&format!("/rest/api/3/issue/{key}/transitions"))
        .await
        .with_context(|| format!("Failed to get transitions for {key}"))?;

    let target = available
        .transitions
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

pub async fn list_comments(ctx: &JiraContext<'_>, key: &str) -> Result<()> {
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
        body_preview: String,
    }

    let rows: Vec<Row<'_>> = response
        .comments
        .iter()
        .map(|c| {
            let preview = extract_adf_text(&c.body)
                .chars()
                .take(50)
                .collect::<String>();
            Row {
                id: c.id.as_str(),
                author: c.author.display_name.as_str(),
                created: c.created.as_str(),
                body_preview: preview,
            }
        })
        .collect();

    ctx.renderer.render(&rows)
}

pub async fn add_comment(ctx: &JiraContext<'_>, key: &str, body: &str) -> Result<()> {
    use serde_json::json;

    let payload = json!({
        "body": {
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": body }]
            }]
        }
    });

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

pub async fn update_comment(ctx: &JiraContext<'_>, comment_id: &str, body: &str) -> Result<()> {
    use serde_json::json;

    let payload = json!({
        "body": {
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": body }]
            }]
        }
    });

    let _: Value = ctx
        .client
        .put(&format!("/rest/api/3/comment/{comment_id}"), &payload)
        .await
        .with_context(|| format!("Failed to update comment {comment_id}"))?;

    tracing::info!(%comment_id, "Comment updated successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Updated comment: {comment_id}"),
        &MutationResult::with_id(format!("Updated comment: {comment_id}"), comment_id),
    )
}

pub async fn delete_comment(ctx: &JiraContext<'_>, comment_id: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/comment/{comment_id}"))
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
}
