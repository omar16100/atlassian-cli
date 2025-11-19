use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

use super::utils::JiraContext;

// List audit records
pub async fn list_audit_records(
    ctx: &JiraContext<'_>,
    from: Option<&str>,
    to: Option<&str>,
    filter: Option<&str>,
    limit: Option<usize>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct AuditResponse {
        records: Vec<AuditRecord>,
    }

    #[derive(Deserialize)]
    struct AuditRecord {
        id: i64,
        summary: String,
        #[serde(rename = "objectItem")]
        object_item: ObjectItem,
        #[serde(rename = "authorKey")]
        author_key: Option<String>,
        created: String,
        category: String,
    }

    #[derive(Deserialize)]
    struct ObjectItem {
        name: Option<String>,
        #[serde(rename = "typeName")]
        type_name: Option<String>,
    }

    let mut query_params = Vec::new();

    if let Some(f) = from {
        query_params.push(format!("from={}", f));
    }

    if let Some(t) = to {
        query_params.push(format!("to={}", t));
    }

    if let Some(flt) = filter {
        query_params.push(format!("filter={}", flt));
    }

    if let Some(l) = limit {
        query_params.push(format!("limit={}", l));
    }

    let query_string = if query_params.is_empty() {
        String::new()
    } else {
        format!("?{}", query_params.join("&"))
    };

    let response: AuditResponse = ctx
        .client
        .get(&format!("/rest/api/3/auditing/record{}", query_string))
        .await
        .context("Failed to list audit records")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        summary: &'a str,
        category: &'a str,
        object_type: &'a str,
        object_name: &'a str,
        author: &'a str,
        created: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .records
        .iter()
        .map(|r| Row {
            id: r.id,
            summary: r.summary.as_str(),
            category: r.category.as_str(),
            object_type: r.object_item.type_name.as_deref().unwrap_or(""),
            object_name: r.object_item.name.as_deref().unwrap_or(""),
            author: r.author_key.as_deref().unwrap_or(""),
            created: r.created.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Export audit records
pub async fn export_audit_records(
    ctx: &JiraContext<'_>,
    from: Option<&str>,
    to: Option<&str>,
    filter: Option<&str>,
    output: &std::path::PathBuf,
    format: ExportFormat,
) -> Result<()> {
    #[derive(Deserialize, Serialize)]
    struct AuditResponse {
        records: Vec<serde_json::Value>,
    }

    let mut query_params = Vec::new();

    if let Some(f) = from {
        query_params.push(format!("from={}", f));
    }

    if let Some(t) = to {
        query_params.push(format!("to={}", t));
    }

    if let Some(flt) = filter {
        query_params.push(format!("filter={}", flt));
    }

    let query_string = if query_params.is_empty() {
        String::new()
    } else {
        format!("?{}", query_params.join("&"))
    };

    let response: AuditResponse = ctx
        .client
        .get(&format!("/rest/api/3/auditing/record{}", query_string))
        .await
        .context("Failed to export audit records")?;

    match format {
        ExportFormat::Json => {
            let json_str = serde_json::to_string_pretty(&response.records)?;
            fs::write(output, json_str)?;
        }
        ExportFormat::Csv => {
            let mut wtr = csv::Writer::from_path(output)?;

            // Write header
            wtr.write_record([
                "id",
                "summary",
                "category",
                "object_type",
                "object_name",
                "author",
                "created",
            ])?;

            // Write rows
            for record in &response.records {
                let id = record
                    .get("id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    .to_string();
                let summary = record.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                let category = record
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let object_type = record
                    .get("objectItem")
                    .and_then(|o| o.get("typeName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let object_name = record
                    .get("objectItem")
                    .and_then(|o| o.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let author = record
                    .get("authorKey")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let created = record.get("created").and_then(|v| v.as_str()).unwrap_or("");

                wtr.write_record([
                    id.as_str(),
                    summary,
                    category,
                    object_type,
                    object_name,
                    author,
                    created,
                ])?;
            }

            wtr.flush()?;
        }
    }

    println!(
        "✅ Exported {} audit records to {}",
        response.records.len(),
        output.display()
    );
    Ok(())
}

#[derive(Debug, Clone)]
pub enum ExportFormat {
    Json,
    Csv,
}
