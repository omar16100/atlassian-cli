use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::{JsmContext, KbArticle};

/// Search knowledge base articles.
pub async fn search_articles(
    ctx: &JsmContext<'_>,
    query: &str,
    servicedesk_id: Option<i64>,
    limit: usize,
) -> Result<()> {
    #[derive(Deserialize)]
    struct ArticleList {
        values: Vec<KbArticle>,
    }

    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("query", query);
    serializer.append_pair("limit", &limit.min(50).to_string());

    let path = if let Some(sd_id) = servicedesk_id {
        format!(
            "/rest/servicedeskapi/servicedesk/{sd_id}/knowledgebase/article?{}",
            serializer.finish()
        )
    } else {
        format!(
            "/rest/servicedeskapi/knowledgebase/article?{}",
            serializer.finish()
        )
    };

    tracing::debug!("Searching knowledge base for: {}", query);
    let response: ArticleList = ctx
        .client
        .get(&path)
        .await
        .context("Failed to search knowledge base")?;

    #[derive(Serialize)]
    struct Row<'a> {
        title: &'a str,
        excerpt: &'a str,
        source_type: &'a str,
        page_id: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|article| Row {
            title: &article.title,
            excerpt: &article.excerpt,
            source_type: article
                .source
                .as_ref()
                .map(|s| s.source_type.as_str())
                .unwrap_or(""),
            page_id: article
                .source
                .as_ref()
                .and_then(|s| s.page_id.as_deref())
                .unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No articles found for query: {}", query);
    }

    tracing::info!("Found {} articles for query: {}", rows.len(), query);
    ctx.renderer
        .render_list_or_empty(&rows, "No articles found")
}
