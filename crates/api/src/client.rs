use anyhow::Result;

/// Base Atlassian HTTP client
pub struct AtlassianClient {
    client: reqwest::Client,
    base_url: String,
}

impl AtlassianClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("atlassiancli/0.1.0")
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.into(),
        })
    }
}
