use atlassiancli_api::ApiClient;
use atlassiancli_output::OutputRenderer;

pub struct JiraContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}
