use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::utils::JiraContext;

// Issue CRUD Operations

pub async fn search_issues(ctx: &JiraContext<'_>, jql: &str, limit: usize) -> Result<()> {
    #[derive(Deserialize)]
    struct SearchResponse {
        issues: Vec<Issue>,
        #[serde(rename = "isLast")]
        is_last: Option<bool>,
        #[serde(rename = "nextPageToken")]
        next_page_token: Option<String>,
    }

    let max_results = limit.min(1000);
    let query = format!(
        "/rest/api/3/search/jql?jql={}&maxResults={}&fields=key,summary,status,assignee,issuetype",
        urlencoding::encode(jql),
        max_results
    );

    let response: SearchResponse = ctx
        .client
        .get(&query)
        .await
        .context("Failed to execute search")?;

    if response.issues.is_empty() {
        tracing::info!("No issues matched the provided JQL.");
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

    #[derive(Serialize)]
    struct IssueDetails<'a> {
        key: &'a str,
        summary: &'a str,
        status: &'a str,
        description: &'a str,
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
        description: issue.fields.description.as_deref().unwrap_or(""),
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

pub async fn create_issue(
    ctx: &JiraContext<'_>,
    project: &str,
    issue_type: &str,
    summary: &str,
    description: Option<&str>,
    assignee: Option<&str>,
    priority: Option<&str>,
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
    println!("✅ Created issue: {}", response.key);
    Ok(())
}

pub async fn update_issue(
    ctx: &JiraContext<'_>,
    key: &str,
    summary: Option<&str>,
    description: Option<&str>,
    priority: Option<&str>,
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

    let payload = json!({ "fields": fields });

    let _: Value = ctx
        .client
        .put(&format!("/rest/api/3/issue/{key}"), &payload)
        .await
        .with_context(|| format!("Failed to update issue {key}"))?;

    tracing::info!(%key, "Issue updated successfully");
    println!("✅ Updated issue: {}", key);
    Ok(())
}

pub async fn delete_issue(ctx: &JiraContext<'_>, key: &str, force: bool) -> Result<()> {
    if !force {
        println!("⚠️  About to delete issue: {}", key);
        println!("Use --force to confirm deletion");
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/issue/{key}"))
        .await
        .with_context(|| format!("Failed to delete issue {key}"))?;

    tracing::info!(%key, "Issue deleted successfully");
    println!("✅ Deleted issue: {}", key);
    Ok(())
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
    println!("✅ Transitioned {} to: {}", key, target.name);
    Ok(())
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
    println!("✅ Assigned {} to: {}", key, assignee);
    Ok(())
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
    println!("✅ Unassigned: {}", key);
    Ok(())
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
    println!("✅ Added watcher to {}: {}", key, user);
    Ok(())
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
    println!("✅ Removed watcher from {}: {}", key, user);
    Ok(())
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
    println!("✅ Linked {} to {} ({})", from, to, link_type);
    Ok(())
}

pub async fn delete_link(ctx: &JiraContext<'_>, link_id: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/issueLink/{link_id}"))
        .await
        .with_context(|| format!("Failed to delete link {link_id}"))?;

    tracing::info!(%link_id, "Issue link deleted successfully");
    println!("✅ Deleted link: {}", link_id);
    Ok(())
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
            let preview = format!("{:?}", c.body).chars().take(50).collect::<String>();
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
    println!("✅ Added comment to: {}", key);
    Ok(())
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
    println!("✅ Updated comment: {}", comment_id);
    Ok(())
}

pub async fn delete_comment(ctx: &JiraContext<'_>, comment_id: &str) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!("/rest/api/3/comment/{comment_id}"))
        .await
        .with_context(|| format!("Failed to delete comment {comment_id}"))?;

    tracing::info!(%comment_id, "Comment deleted successfully");
    println!("✅ Deleted comment: {}", comment_id);
    Ok(())
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
    description: Option<String>,
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
