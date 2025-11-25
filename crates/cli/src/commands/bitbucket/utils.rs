use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use url::Url;

pub struct BitbucketContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}

/// Extract Bitbucket workspace from a URL.
/// Supports:
/// - https://bitbucket.org/{workspace}
/// - https://bitbucket.org/{workspace}/...
pub fn extract_workspace_from_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if parsed.host_str() == Some("bitbucket.org") {
        let path = parsed.path().trim_start_matches('/');
        let workspace = path.split('/').next()?;
        if !workspace.is_empty() {
            return Some(workspace.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_workspace_from_bitbucket_url() {
        assert_eq!(
            extract_workspace_from_url("https://bitbucket.org/myworkspace"),
            Some("myworkspace".to_string())
        );
    }

    #[test]
    fn test_extract_workspace_from_bitbucket_url_with_path() {
        assert_eq!(
            extract_workspace_from_url("https://bitbucket.org/myworkspace/some/repo"),
            Some("myworkspace".to_string())
        );
    }

    #[test]
    fn test_extract_workspace_from_non_bitbucket_url() {
        assert_eq!(
            extract_workspace_from_url("https://example.atlassian.net"),
            None
        );
    }

    #[test]
    fn test_extract_workspace_from_root_url() {
        assert_eq!(extract_workspace_from_url("https://bitbucket.org/"), None);
    }

    #[test]
    fn test_extract_workspace_from_empty_path() {
        assert_eq!(extract_workspace_from_url("https://bitbucket.org"), None);
    }
}
