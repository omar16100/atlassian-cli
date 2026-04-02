use anyhow::Context;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;

pub struct JiraContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}

impl JiraContext<'_> {
    pub async fn verify_auth(&self) -> anyhow::Result<()> {
        let _: serde_json::Value = self
            .client
            .get("/rest/api/3/myself")
            .await
            .context("Failed to verify Jira access. Run: atlassian-cli auth test")?;
        Ok(())
    }
}
