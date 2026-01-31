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
            .context("Authentication may be expired or invalid. Run: atlassian-cli auth test")?;
        Ok(())
    }
}
