use anyhow::{Context, Result};
use atlassian_cli_bulk::BulkExecutor;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use super::utils::JiraContext;

// Bulk transition issues
pub async fn bulk_transition(
    ctx: &JiraContext<'_>,
    jql: &str,
    transition: &str,
    dry_run: bool,
    concurrency: usize,
) -> Result<()> {
    // Search for issues
    let issue_keys = search_issue_keys(ctx, jql).await?;

    if issue_keys.is_empty() {
        println!("No issues matched the JQL query");
        return Ok(());
    }

    println!("Found {} issues to transition", issue_keys.len());

    if dry_run {
        println!("🔍 Dry run mode - no changes will be made:");
        for key in &issue_keys {
            println!("  Would transition: {}", key);
        }
        return Ok(());
    }

    // Get transition ID
    let transition_id = get_transition_id(ctx, &issue_keys[0], transition).await?;

    let executor = BulkExecutor::new(concurrency, dry_run);
    let client = ctx.client.clone();

    executor
        .run(issue_keys, move |key| {
            let client = client.clone();
            let transition_id = transition_id.clone();
            async move {
                let payload = json!({ "transition": { "id": transition_id } });
                let _: Value = client
                    .post(&format!("/rest/api/3/issue/{key}/transitions"), &payload)
                    .await
                    .with_context(|| format!("Failed to transition issue {key}"))?;
                tracing::info!(%key, "Transitioned successfully");
                Ok(())
            }
        })
        .await?;

    println!("✅ Bulk transition completed");
    Ok(())
}

// Bulk assign issues
pub async fn bulk_assign(
    ctx: &JiraContext<'_>,
    jql: &str,
    assignee: &str,
    dry_run: bool,
    concurrency: usize,
) -> Result<()> {
    let issue_keys = search_issue_keys(ctx, jql).await?;

    if issue_keys.is_empty() {
        println!("No issues matched the JQL query");
        return Ok(());
    }

    println!("Found {} issues to assign", issue_keys.len());

    if dry_run {
        println!("🔍 Dry run mode - no changes will be made:");
        for key in &issue_keys {
            println!("  Would assign {} to {}", key, assignee);
        }
        return Ok(());
    }

    let executor = BulkExecutor::new(concurrency, dry_run);
    let client = ctx.client.clone();
    let assignee = assignee.to_string();

    executor
        .run(issue_keys, move |key| {
            let client = client.clone();
            let assignee = assignee.clone();
            async move {
                let payload = json!({ "accountId": assignee });
                let _: Value = client
                    .put(&format!("/rest/api/3/issue/{key}/assignee"), &payload)
                    .await
                    .with_context(|| format!("Failed to assign issue {key}"))?;
                tracing::info!(%key, %assignee, "Assigned successfully");
                Ok(())
            }
        })
        .await?;

    println!("✅ Bulk assign completed");
    Ok(())
}

// Bulk label operations
pub async fn bulk_label(
    ctx: &JiraContext<'_>,
    jql: &str,
    action: LabelAction,
    labels: Vec<String>,
    dry_run: bool,
    concurrency: usize,
) -> Result<()> {
    let issue_keys = search_issue_keys(ctx, jql).await?;

    if issue_keys.is_empty() {
        println!("No issues matched the JQL query");
        return Ok(());
    }

    println!("Found {} issues to label", issue_keys.len());

    if dry_run {
        println!("🔍 Dry run mode - no changes will be made:");
        for key in &issue_keys {
            println!("  Would {:?} labels {:?} on {}", action, labels, key);
        }
        return Ok(());
    }

    let executor = BulkExecutor::new(concurrency, dry_run);
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
                    .put(&format!("/rest/api/3/issue/{key}"), &payload)
                    .await
                    .with_context(|| format!("Failed to update labels for {key}"))?;

                tracing::info!(%key, "Labels updated successfully");
                Ok(())
            }
        })
        .await?;

    println!("✅ Bulk label operation completed");
    Ok(())
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
    let field_list = if fields.is_empty() {
        "*all".to_string()
    } else {
        fields.join(",")
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
        .post("/rest/api/3/search", &payload)
        .await
        .context("Failed to search issues")?;

    if response.issues.is_empty() {
        println!("No issues matched the JQL query");
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

    let executor = BulkExecutor::new(concurrency, dry_run);
    let client = ctx.client.clone();
    let project = project.to_string();

    executor
        .run(issues, move |issue| {
            let client = client.clone();
            let project = project.clone();
            async move {
                let mut fields = json!({
                    "project": { "key": project },
                    "issuetype": { "name": issue.issue_type },
                    "summary": issue.summary,
                });

                if let Some(desc) = issue.description {
                    fields["description"] = json!({
                        "type": "doc",
                        "version": 1,
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": desc }]
                        }]
                    });
                }

                if let Some(assignee) = issue.assignee {
                    fields["assignee"] = json!({ "id": assignee });
                }

                if let Some(priority) = issue.priority {
                    fields["priority"] = json!({ "name": priority });
                }

                if !issue.labels.is_empty() {
                    fields["labels"] = json!(issue.labels);
                }

                let payload = json!({ "fields": fields });

                let response: CreateResponse = client
                    .post("/rest/api/3/issue", &payload)
                    .await
                    .context("Failed to create issue")?;

                tracing::info!(key = %response.key, "Issue created successfully");
                Ok(())
            }
        })
        .await?;

    println!("✅ Bulk import completed");
    Ok(())
}

// Helper functions

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
        .post("/rest/api/3/search", &payload)
        .await
        .context("Failed to search issues")?;

    Ok(response.issues.into_iter().map(|i| i.key).collect())
}

async fn get_transition_id(ctx: &JiraContext<'_>, key: &str, transition: &str) -> Result<String> {
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
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(transition) || t.id == transition)
        .ok_or_else(|| anyhow::anyhow!("Transition '{}' not found", transition))?;

    Ok(target.id)
}

// Data structures

#[derive(Debug, Clone)]
pub enum LabelAction {
    Add,
    Remove,
    Set,
}

#[derive(Debug, Clone)]
pub enum ExportFormat {
    Json,
    Csv,
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
}

#[derive(Deserialize)]
struct CreateResponse {
    key: String,
}
