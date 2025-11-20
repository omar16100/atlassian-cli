use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::ConfluenceContext;

// Get page views
pub async fn get_page_views(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    from_date: Option<&str>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct ViewsResponse {
        count: i64,
    }

    let mut query_params = Vec::new();

    if let Some(from) = from_date {
        query_params.push(format!("fromDate={}", from));
    }

    let query_string = if query_params.is_empty() {
        String::new()
    } else {
        format!("?{}", query_params.join("&"))
    };

    let response: ViewsResponse = ctx
        .client
        .get(&format!(
            "/wiki/rest/api/analytics/content/{}/views{}",
            page_id, query_string
        ))
        .await
        .with_context(|| format!("Failed to get views for page {}", page_id))?;

    #[derive(Serialize)]
    struct Row {
        page_id: String,
        view_count: i64,
    }

    let row = Row {
        page_id: page_id.to_string(),
        view_count: response.count,
    };

    ctx.renderer.render(&[row])
}

// Get space analytics
pub async fn get_space_analytics(ctx: &ConfluenceContext<'_>, space_key: &str) -> Result<()> {
    // Get space content count using CQL
    let pages_cql = format!("space = \"{}\" AND type = page", space_key);
    let blogs_cql = format!("space = \"{}\" AND type = blogpost", space_key);

    #[derive(Deserialize)]
    struct SearchResponse {
        size: i64,
    }

    let pages_response: SearchResponse = ctx
        .client
        .get(&format!(
            "/wiki/rest/api/content/search?cql={}",
            urlencoding::encode(&pages_cql)
        ))
        .await
        .context("Failed to get page count")?;

    let blogs_response: SearchResponse = ctx
        .client
        .get(&format!(
            "/wiki/rest/api/content/search?cql={}",
            urlencoding::encode(&blogs_cql)
        ))
        .await
        .context("Failed to get blog count")?;

    #[derive(Serialize)]
    struct Row<'a> {
        space_key: &'a str,
        total_pages: i64,
        total_blogs: i64,
    }

    let row = Row {
        space_key,
        total_pages: pages_response.size,
        total_blogs: blogs_response.size,
    };

    ctx.renderer.render(&[row])
}
