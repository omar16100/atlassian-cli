use anyhow::Context;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use url::Url;

pub struct BitbucketContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
    /// true if using Bearer auth (access tokens), false for Basic auth
    pub is_bearer: bool,
}

impl BitbucketContext<'_> {
    pub async fn verify_auth(&self) -> anyhow::Result<()> {
        // Bearer tokens (access tokens) can't use /2.0/user — use /2.0/workspaces
        let endpoint = if self.is_bearer {
            "/2.0/workspaces"
        } else {
            "/2.0/user"
        };
        let _: serde_json::Value = self.client.get(endpoint).await.context(
            "Failed to verify Bitbucket access. Run: atlassian-cli auth test --bitbucket",
        )?;
        Ok(())
    }
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

/// Percent-encode a ref name for interpolation into a URL path.
///
/// Git permits `#` in a branch name, and `#` starts a URL fragment. Interpolated
/// raw, a branch called `main#old` produces a path the client resolves to
/// `.../refs/branches/main`, with `old` as a fragment that is never sent. A
/// `DELETE` aimed at `main#old` therefore lands on `main`, straight through the
/// protected-name check, while the confirmation prompt still says `main#old`.
///
/// `/` is deliberately preserved: `feature/login` is an ordinary branch name and
/// Bitbucket expects those slashes as real path separators. Everything outside
/// the RFC 3986 unreserved set is encoded, which covers `#` and the `%` that
/// would otherwise let a name like `foo%23` be decoded by the server into a
/// different ref.
pub fn encode_ref_path(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this exists for: `#` truncates the path at the client, so the
    /// request lands on a different — and possibly protected — ref.
    #[test]
    fn hash_in_a_branch_name_is_encoded() {
        assert_eq!(encode_ref_path("main#old"), "main%23old");
        assert_ne!(encode_ref_path("main#old"), "main#old");
    }

    #[test]
    fn slashes_are_preserved_as_path_separators() {
        assert_eq!(encode_ref_path("feature/login"), "feature/login");
        assert_eq!(encode_ref_path("release/2026.09"), "release/2026.09");
    }

    #[test]
    fn percent_is_encoded_so_the_server_cannot_re_decode_it() {
        assert_eq!(encode_ref_path("foo%23"), "foo%2523");
    }

    #[test]
    fn ordinary_names_are_unchanged() {
        for name in ["main", "develop", "feature/a-b_c.d~e"] {
            assert_eq!(encode_ref_path(name), name);
        }
    }

    #[test]
    fn non_ascii_is_utf8_percent_encoded() {
        assert_eq!(encode_ref_path("função"), "fun%C3%A7%C3%A3o");
    }

    /// Guards the actual consequence, not just the string transform: after
    /// encoding, resolving the path must still target the original ref.
    #[test]
    fn encoded_path_resolves_to_the_intended_ref() {
        let base = Url::parse("https://api.bitbucket.org/").unwrap();
        let raw = base
            .join("2.0/repositories/w/r/refs/branches/main#old")
            .unwrap();
        assert_eq!(raw.path(), "/2.0/repositories/w/r/refs/branches/main");

        let encoded = base
            .join(&format!(
                "2.0/repositories/w/r/refs/branches/{}",
                encode_ref_path("main#old")
            ))
            .unwrap();
        assert_eq!(
            encoded.path(),
            "/2.0/repositories/w/r/refs/branches/main%23old"
        );
        assert_eq!(encoded.fragment(), None);
    }

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
