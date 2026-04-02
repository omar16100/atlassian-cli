use anyhow::Context;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;

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
