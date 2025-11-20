use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;

pub struct BitbucketContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}
