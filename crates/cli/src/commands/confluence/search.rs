use anyhow::{anyhow, Context, Result};
use atlassian_cli_api::pagination::{fetch_paged, ConfluencePage, PageLimits};
use serde::{Deserialize, Serialize};

use super::utils::ConfluenceContext;
use crate::query::CqlBuilder;

// Search using CQL
pub async fn search_cql(
    ctx: &ConfluenceContext<'_>,
    cql: &str,
    limit: Option<usize>,
) -> Result<()> {
    // Only `id` is guaranteed; `title`/`type` may be null or absent for some
    // content types, which would otherwise abort the parse.
    #[derive(Deserialize)]
    struct SearchResult {
        id: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(rename = "type", default)]
        content_type: Option<String>,
    }

    // With no flag, retain the existing one-page default. `--limit` is a total
    // result count, not a per-request page size; 0 asks for everything.
    let limit = limit.unwrap_or(25);
    let page_size = if limit == 0 { 250 } else { limit.min(250) };
    let query = format!(
        "/wiki/rest/api/content/search?cql={}&limit={page_size}",
        atlassian_cli_api::query::encode_query_component(cql)
    );

    let (results, page) = fetch_paged::<ConfluencePage<SearchResult>>(
        &ctx.client,
        &query,
        PageLimits::from_cli_limit(limit),
    )
    .await
    .context("Failed to search with CQL")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        title: &'a str,
        content_type: &'a str,
    }

    if results.is_empty() {
        ctx.verify_auth().await?;
        tracing::info!("No results found");
    }

    if page.truncated {
        if limit == 0 || results.len() < limit {
            eprintln!(
                "warning: showing {} results; pagination stopped before completion.",
                results.len()
            );
        } else {
            eprintln!(
                "warning: showing {} results; more match this query. Raise --limit, or use --limit 0 for all.",
                results.len()
            );
        }
    }

    let rows: Vec<Row<'_>> = results
        .iter()
        .map(|r| Row {
            id: r.id.as_str(),
            title: r.title.as_deref().unwrap_or(""),
            content_type: r.content_type.as_deref().unwrap_or(""),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Text search
pub async fn search_text(
    ctx: &ConfluenceContext<'_>,
    query: &str,
    limit: Option<usize>,
) -> Result<()> {
    let cql = CqlBuilder::new().contains("text", query).finish();
    search_cql(ctx, &cql, limit).await
}

// Search in space
pub async fn search_in_space(
    ctx: &ConfluenceContext<'_>,
    space_key: &str,
    query: &str,
    limit: Option<usize>,
) -> Result<()> {
    let cql = CqlBuilder::new()
        .eq("space", space_key)
        .contains("text", query)
        .finish();
    search_cql(ctx, &cql, limit).await
}

// Search using filter parameters
#[allow(clippy::too_many_arguments)]
pub async fn search_params(
    ctx: &ConfluenceContext<'_>,
    space: Option<&str>,
    r#type: Option<&str>,
    creator: Option<&str>,
    label: &[String],
    title: Option<&str>,
    text: Option<&str>,
    show_query: bool,
    limit: usize,
) -> Result<()> {
    let mut builder = CqlBuilder::new();

    // Add filters conditionally
    if let Some(s) = space {
        builder = builder.eq("space", s);
    }
    if let Some(t) = r#type {
        builder = builder.eq("type", t);
    }
    if let Some(c) = creator {
        builder = builder.eq("creator", c);
    }
    if !label.is_empty() {
        builder = builder.in_list("label", label);
    }
    if let Some(t) = title {
        builder = builder.contains("title", t);
    }
    if let Some(txt) = text {
        builder = builder.contains("text", txt);
    }

    let cql = builder.finish();
    if cql.is_empty() {
        return Err(anyhow!(
            "No search criteria provided. Use filter flags to build a query"
        ));
    }

    if show_query {
        println!("CQL Query: {}", cql);
        println!();
    }

    search_cql(ctx, &cql, Some(limit)).await
}
