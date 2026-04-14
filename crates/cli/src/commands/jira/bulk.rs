use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use atlassian_cli_bulk::BulkExecutor;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::issues::{check_field_collisions, plain_text_adf};
use super::utils::JiraContext;
use crate::commands::common::{render_success, MutationResult};

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
        fields["description"] = plain_text_adf(desc);
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
        println!("No issues found matching query");
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

    render_success(
        ctx.renderer,
        "✅ Bulk transition completed",
        &MutationResult::new("Bulk transition completed"),
    )
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
        println!("No issues found matching query");
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

    render_success(
        ctx.renderer,
        "✅ Bulk assign completed",
        &MutationResult::new("Bulk assign completed"),
    )
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
        println!("No issues found matching query");
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
    #[serde(default)]
    pub custom_fields: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
struct CreateResponse {
    key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
