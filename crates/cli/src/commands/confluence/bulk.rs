use anyhow::{Context, Result};
use atlassian_cli_bulk::BulkExecutor;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use super::utils::ConfluenceContext;

// Bulk delete pages
pub async fn bulk_delete_pages(
    ctx: &ConfluenceContext<'_>,
    cql: &str,
    dry_run: bool,
    concurrency: usize,
) -> Result<()> {
    let page_ids = search_page_ids(ctx, cql).await?;

    if page_ids.is_empty() {
        println!("No pages matched the CQL query");
        return Ok(());
    }

    println!("Found {} pages to delete", page_ids.len());

    if dry_run {
        println!("🔍 Dry run mode - no changes will be made:");
        for id in &page_ids {
            println!("  Would delete: {}", id);
        }
        return Ok(());
    }

    let executor = BulkExecutor::new(concurrency, dry_run);
    let client = ctx.client.clone();

    executor
        .run(page_ids, move |id| {
            let client = client.clone();
            async move {
                let _: Value = client
                    .delete(&format!("/wiki/api/v2/pages/{}", id))
                    .await
                    .with_context(|| format!("Failed to delete page {}", id))?;
                tracing::info!(%id, "Page deleted successfully");
                Ok(())
            }
        })
        .await?;

    println!("✅ Bulk delete completed");
    Ok(())
}

// Bulk add labels
pub async fn bulk_add_labels(
    ctx: &ConfluenceContext<'_>,
    cql: &str,
    labels: Vec<String>,
    dry_run: bool,
    concurrency: usize,
) -> Result<()> {
    let page_ids = search_page_ids(ctx, cql).await?;

    if page_ids.is_empty() {
        println!("No pages matched the CQL query");
        return Ok(());
    }

    println!("Found {} pages to label", page_ids.len());

    if dry_run {
        println!("🔍 Dry run mode - no changes will be made:");
        for id in &page_ids {
            println!("  Would add labels {:?} to page {}", labels, id);
        }
        return Ok(());
    }

    let executor = BulkExecutor::new(concurrency, dry_run);
    let client = ctx.client.clone();

    executor
        .run(page_ids, move |id| {
            let client = client.clone();
            let labels = labels.clone();
            async move {
                let label_objects: Vec<_> = labels
                    .iter()
                    .map(|l| json!({"prefix": "global", "name": l}))
                    .collect();

                let _: Value = client
                    .post(&format!("/wiki/rest/api/content/{}/label", id), &label_objects)
                    .await
                    .with_context(|| format!("Failed to add labels to page {}", id))?;

                tracing::info!(%id, "Labels added successfully");
                Ok(())
            }
        })
        .await?;

    println!("✅ Bulk label operation completed");
    Ok(())
}

// Bulk export pages
pub async fn bulk_export_pages(
    ctx: &ConfluenceContext<'_>,
    cql: &str,
    output: &PathBuf,
    format: ExportFormat,
) -> Result<()> {
    #[derive(Deserialize)]
    struct SearchResponse {
        results: Vec<SearchResult>,
    }

    #[derive(Deserialize)]
    struct SearchResult {
        content: Value,
    }

    let query_string = format!("cql={}", urlencoding::encode(cql));

    let response: SearchResponse = ctx
        .client
        .get(&format!("/wiki/rest/api/content/search?{}&expand=body.storage", query_string))
        .await
        .context("Failed to search pages")?;

    if response.results.is_empty() {
        println!("No pages matched the CQL query");
        return Ok(());
    }

    println!("Found {} pages to export", response.results.len());

    let pages: Vec<Value> = response.results.into_iter().map(|r| r.content).collect();

    match format {
        ExportFormat::Json => {
            let json_str = serde_json::to_string_pretty(&pages)?;
            fs::write(output, json_str)?;
        }
        ExportFormat::Csv => {
            let mut wtr = csv::Writer::from_path(output)?;

            // Write header
            wtr.write_record(["id", "title", "type", "space"])?;

            // Write rows
            for page in &pages {
                let id = page.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let title = page.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let page_type = page.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let space = page
                    .get("space")
                    .and_then(|s| s.get("key"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                wtr.write_record([id, title, page_type, space])?;
            }

            wtr.flush()?;
        }
    }

    println!("✅ Exported {} pages to {}", pages.len(), output.display());
    Ok(())
}

// Helper function to search for page IDs using CQL
// Note: Currently limited to 1000 results. TODO: Implement cursor-based pagination for larger result sets
async fn search_page_ids(ctx: &ConfluenceContext<'_>, cql: &str) -> Result<Vec<String>> {
    const MAX_RESULTS: usize = 1000;

    #[derive(Deserialize)]
    struct SearchResponse {
        results: Vec<SearchResult>,
    }

    #[derive(Deserialize)]
    struct SearchResult {
        content: Content,
    }

    #[derive(Deserialize)]
    struct Content {
        id: String,
    }

    let query_string = format!("cql={}&limit={}", urlencoding::encode(cql), MAX_RESULTS);

    let response: SearchResponse = ctx
        .client
        .get(&format!("/wiki/rest/api/content/search?{}", query_string))
        .await
        .context("Failed to search pages")?;

    if response.results.len() >= MAX_RESULTS {
        tracing::warn!(
            "Search returned maximum results ({}). Some pages may be excluded. Consider using more specific CQL.",
            MAX_RESULTS
        );
    }

    Ok(response.results.into_iter().map(|r| r.content.id).collect())
}

#[derive(Debug, Clone)]
pub enum ExportFormat {
    Json,
    Csv,
}
