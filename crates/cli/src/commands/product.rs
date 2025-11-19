use std::fmt;

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug, Default, Clone)]
pub struct ProductArgs {
    /// Additional arguments that will be passed once product-specific commands are implemented.
    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    pub passthrough: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum ProductKind {
    Jira,
    Confluence,
    Bitbucket,
    Jsm,
    Opsgenie,
    Bamboo,
}

impl ProductKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProductKind::Jira => "jira",
            ProductKind::Confluence => "confluence",
            ProductKind::Bitbucket => "bitbucket",
            ProductKind::Jsm => "jsm",
            ProductKind::Opsgenie => "opsgenie",
            ProductKind::Bamboo => "bamboo",
        }
    }
}

impl fmt::Display for ProductKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn handle(product: ProductKind, args: &ProductArgs) -> Result<()> {
    if args.passthrough.is_empty() {
        tracing::info!(target: "atlassiancli", product = %product, "Product support coming soon.");
    } else {
        tracing::warn!(
            target: "atlassiancli",
            product = %product,
            args = ?args.passthrough,
            "Product command is not implemented yet"
        );
    }
    Ok(())
}
