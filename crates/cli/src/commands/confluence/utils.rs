use anyhow::{anyhow, Context};
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use serde::Deserialize;

use crate::query::UrlParamsBuilder;

pub struct ConfluenceContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}

impl ConfluenceContext<'_> {
    pub async fn verify_auth(&self) -> anyhow::Result<()> {
        let _: serde_json::Value = self
            .client
            .get("/wiki/rest/api/user/current")
            .await
            .context("Failed to verify Confluence access. Run: atlassian-cli auth test")?;
        Ok(())
    }
}

/// Resolve a Confluence space key to its numeric v2 space ID via
/// `GET /wiki/api/v2/spaces?keys=<key>`. Errors if no space matches.
pub(crate) async fn resolve_space_id(
    ctx: &ConfluenceContext<'_>,
    key: &str,
) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct SpacesResponse {
        results: Vec<SpaceRef>,
    }
    #[derive(Deserialize)]
    struct SpaceRef {
        id: String,
    }

    let query = UrlParamsBuilder::new().add("keys", key).finish();
    let resp: SpacesResponse = ctx
        .client
        .get(&format!("/wiki/api/v2/spaces?{query}"))
        .await
        .with_context(|| format!("Failed to look up space '{key}'"))?;

    resp.results
        .into_iter()
        .next()
        .map(|s| s.id)
        .ok_or_else(|| anyhow!("No Confluence space found with key '{key}'"))
}

/// The largest `limit` that the Confluence v2 list endpoints accept.
///
/// Source: the `limit` parameter of `GET /spaces`, `GET /pages` and
/// `GET /blogposts` in the Confluence Cloud REST API v2 specification
/// (`"default": 25, "minimum": 1, "maximum": 250`),
/// <https://dac-static.atlassian.com/cloud/confluence/openapi-v2.v3.json>.
pub(crate) const V2_MAX_PAGE_SIZE: usize = 250;

/// The total a list command returns when `--limit` is not given. It is the
/// server default page size, so a run without the flag stays one request.
pub(crate) const DEFAULT_LIST_LIMIT: usize = 25;

/// Page size for the first request of a v2 list command.
///
/// `--limit` is a total, not a page size. A value above the server maximum is
/// clamped, and `--limit 0` means everything, so it asks for a full page.
pub(crate) fn v2_page_size(limit: usize) -> usize {
    if limit == 0 {
        V2_MAX_PAGE_SIZE
    } else {
        limit.min(V2_MAX_PAGE_SIZE)
    }
}

/// Fetch a v2 collection (`/wiki/api/v2/spaces`, `pages`, `blogposts`) up to a
/// total of `limit` items, and follow `_links.next` between pages.
///
/// `filters` holds the other query parameters, without `limit`. A result that
/// stops before the end of the collection writes a warning to stderr, so a
/// piped `-f json` result stays valid.
pub(crate) async fn fetch_v2_list<T: serde::de::DeserializeOwned>(
    ctx: &ConfluenceContext<'_>,
    collection_path: &str,
    filters: UrlParamsBuilder,
    limit: Option<usize>,
    noun: &str,
) -> atlassian_cli_api::error::Result<Vec<T>> {
    use atlassian_cli_api::pagination::{fetch_paged, ConfluencePage, PageLimits};

    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let page_size = v2_page_size(limit);
    let query = filters.add("limit", &page_size.to_string()).finish();
    let path = format!("{collection_path}?{query}");
    tracing::debug!(%path, limit, page_size, "Listing a Confluence v2 collection");

    let (items, page) =
        fetch_paged::<ConfluencePage<T>>(&ctx.client, &path, PageLimits::from_cli_limit(limit))
            .await?;

    if page.truncated {
        if limit == 0 || items.len() < limit {
            eprintln!(
                "warning: showing {} {noun}; pagination stopped before completion.",
                items.len()
            );
        } else {
            eprintln!(
                "warning: showing {} {noun}; more exist. Raise --limit, or use --limit 0 for all.",
                items.len()
            );
        }
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_is_the_limit_up_to_the_server_maximum() {
        assert_eq!(v2_page_size(1), 1);
        assert_eq!(v2_page_size(25), 25);
        assert_eq!(v2_page_size(250), 250);
        assert_eq!(v2_page_size(251), 250);
        assert_eq!(v2_page_size(10_000), 250);
    }

    /// 0 means everything. A page size of 0 is below the server minimum of 1.
    #[test]
    fn limit_zero_asks_for_a_full_page() {
        assert_eq!(v2_page_size(0), 250);
    }
}
