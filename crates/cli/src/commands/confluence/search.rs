use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::ConfluenceContext;
use crate::query::CqlBuilder;

// Search using CQL
pub async fn search_cql(
    ctx: &ConfluenceContext<'_>,
    cql: &str,
    limit: Option<usize>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct SearchResponse {
        results: Vec<SearchResult>,
    }

    #[derive(Deserialize)]
    struct SearchResult {
        id: String,
        title: String,
        #[serde(rename = "type")]
        content_type: String,
    }

    let mut query_params = vec![format!("cql={}", urlencoding::encode(cql))];

    if let Some(l) = limit {
        query_params.push(format!("limit={}", l));
    }

    let query_string = query_params.join("&");

    let response: SearchResponse = ctx
        .client
        .get(&format!("/wiki/rest/api/content/search?{}", query_string))
        .await
        .context("Failed to search with CQL")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        title: &'a str,
        content_type: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .results
        .iter()
        .map(|r| Row {
            id: r.id.as_str(),
            title: r.title.as_str(),
            content_type: r.content_type.as_str(),
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
    let cql = format!("text ~ \"{}\"", query);
    search_cql(ctx, &cql, limit).await
}

// Search in space
pub async fn search_in_space(
    ctx: &ConfluenceContext<'_>,
    space_key: &str,
    query: &str,
    limit: Option<usize>,
) -> Result<()> {
    let cql = format!("space = \"{}\" AND text ~ \"{}\"", space_key, query);
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
