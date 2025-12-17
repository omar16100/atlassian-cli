use std::process::Command;
use url::Url;

/// Git context detected from local repository
#[derive(Debug, Clone, Default)]
pub struct GitContext {
    pub workspace: Option<String>,
    pub repo_slug: Option<String>,
}

/// Detect workspace and repo from git remote URL
pub fn detect_git_context() -> GitContext {
    // Try to get origin remote URL
    let output = match Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => {
            tracing::debug!("Failed to get git remote URL");
            return GitContext::default();
        }
    };

    let remote_url = match String::from_utf8(output.stdout) {
        Ok(url) => url.trim().to_string(),
        Err(_) => {
            tracing::debug!("Invalid UTF-8 in git remote URL");
            return GitContext::default();
        }
    };

    if remote_url.is_empty() {
        tracing::debug!("Empty git remote URL");
        return GitContext::default();
    }

    // Parse the remote URL
    match parse_git_remote(&remote_url) {
        Some((workspace, repo_slug)) => {
            tracing::debug!(
                workspace = %workspace,
                repo_slug = %repo_slug,
                "Detected git context from remote"
            );
            GitContext {
                workspace: Some(workspace),
                repo_slug: Some(repo_slug),
            }
        }
        None => {
            tracing::debug!(remote_url = %remote_url, "Not a Bitbucket remote");
            GitContext::default()
        }
    }
}

/// Parse git remote URL to extract workspace and repo slug
/// Supports:
/// - HTTPS: https://bitbucket.org/{workspace}/{repo}.git
/// - SSH: git@bitbucket.org:{workspace}/{repo}.git
fn parse_git_remote(url: &str) -> Option<(String, String)> {
    // Try SSH format first: git@bitbucket.org:workspace/repo.git
    if url.starts_with("git@bitbucket.org:") {
        let path = url.strip_prefix("git@bitbucket.org:")?;
        return parse_path_segments(path);
    }

    // Try HTTPS format: https://bitbucket.org/workspace/repo.git
    if let Ok(parsed) = Url::parse(url) {
        if parsed.host_str() == Some("bitbucket.org") {
            let path = parsed.path().trim_start_matches('/');
            return parse_path_segments(path);
        }
    }

    None
}

/// Parse path segments to extract workspace and repo
/// Handles both "workspace/repo.git" and "workspace/repo"
fn parse_path_segments(path: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 {
        let workspace = parts[0];
        let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);

        if !workspace.is_empty() && !repo.is_empty() {
            return Some((workspace.to_string(), repo.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_https_git_remote() {
        assert_eq!(
            parse_git_remote("https://bitbucket.org/ntuclink/blueparrot_ai.git"),
            Some(("ntuclink".to_string(), "blueparrot_ai".to_string()))
        );
    }

    #[test]
    fn test_parse_https_without_git_suffix() {
        assert_eq!(
            parse_git_remote("https://bitbucket.org/myworkspace/myrepo"),
            Some(("myworkspace".to_string(), "myrepo".to_string()))
        );
    }

    #[test]
    fn test_parse_ssh_git_remote() {
        assert_eq!(
            parse_git_remote("git@bitbucket.org:ntuclink/blueparrot_ai.git"),
            Some(("ntuclink".to_string(), "blueparrot_ai".to_string()))
        );
    }

    #[test]
    fn test_parse_ssh_without_git_suffix() {
        assert_eq!(
            parse_git_remote("git@bitbucket.org:myworkspace/myrepo"),
            Some(("myworkspace".to_string(), "myrepo".to_string()))
        );
    }

    #[test]
    fn test_parse_github_remote() {
        assert_eq!(parse_git_remote("https://github.com/user/repo.git"), None);
    }

    #[test]
    fn test_parse_non_bitbucket_ssh() {
        assert_eq!(parse_git_remote("git@github.com:user/repo.git"), None);
    }

    #[test]
    fn test_parse_invalid_url() {
        assert_eq!(parse_git_remote("not-a-url"), None);
    }

    #[test]
    fn test_parse_empty_url() {
        assert_eq!(parse_git_remote(""), None);
    }

    #[test]
    fn test_parse_bitbucket_with_nested_path() {
        // Should extract first two segments only
        assert_eq!(
            parse_git_remote("https://bitbucket.org/workspace/repo/extra/path.git"),
            Some(("workspace".to_string(), "repo".to_string()))
        );
    }
}
