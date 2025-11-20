use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::ConfluenceContext;

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
        content: Content,
    }

    #[derive(Deserialize)]
    struct Content {
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
            id: r.content.id.as_str(),
            title: r.content.title.as_str(),
            content_type: r.content.content_type.as_str(),
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
