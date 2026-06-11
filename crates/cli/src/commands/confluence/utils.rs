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
