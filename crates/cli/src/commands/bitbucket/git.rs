use std::env;
use std::process::Command;
use url::Url;

/// Git context detected from local repository
#[derive(Debug, Clone, Default)]
pub struct GitContext {
    pub workspace: Option<String>,
    pub repo_slug: Option<String>,
}

/// Redact credentials from URLs for safe display.
/// Public alias for use in error messages.
pub fn redact_url(url_str: &str) -> String {
    redact_url_credentials(url_str)
}

/// Redact credentials from URLs for safe logging
/// Replaces userinfo (username/password/token) with [REDACTED]
fn redact_url_credentials(url_str: &str) -> String {
    // Try to parse as URL
    if let Ok(mut url) = Url::parse(url_str) {
        if url.username() != "" || url.password().is_some() {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            // Add marker that credentials were redacted
            return url.as_str().replace("://", "://[REDACTED]@");
        }
        return url.to_string();
    }

    // For SSH URLs like git@host:path, just return as-is (no credentials to redact)
    url_str.to_string()
}

/// Try to get fetch URL for a specific remote name
fn try_remote(name: &str) -> Option<GitContext> {
    let output = Command::new("git")
        .args(["remote", "get-url", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if url.is_empty() {
        return None;
    }
    let (workspace, repo_slug) = parse_git_remote(&url)?;
    tracing::debug!(
        workspace = %workspace,
        repo_slug = %repo_slug,
        remote = name,
        "Detected git context from remote"
    );
    Some(GitContext {
        workspace: Some(workspace),
        repo_slug: Some(repo_slug),
    })
}

/// Get all git remote names (deduplicated)
fn get_remote_names() -> Vec<String> {
    let output = match Command::new("git").arg("remote").output() {
        Ok(output) if output.status.success() => output,
        _ => return vec![],
    };
    String::from_utf8(output.stdout)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Detect workspace and repo from git remote URL.
/// Priority: preferred_remote > origin (if Bitbucket) > first Bitbucket remote.
pub fn detect_git_context(preferred_remote: Option<&str>) -> GitContext {
    // 1. If user configured a preferred remote, try that first
    if let Some(remote_name) = preferred_remote {
        if let Some(ctx) = try_remote(remote_name) {
            return ctx;
        }
        tracing::debug!(
            remote = remote_name,
            "Configured remote not found or not Bitbucket, trying all remotes"
        );
    }

    let remote_names = get_remote_names();
    if remote_names.is_empty() {
        tracing::debug!("No git remotes found");
        return GitContext::default();
    }

    // 2. Try "origin" first if it exists
    if remote_names.iter().any(|n| n == "origin") {
        if let Some(ctx) = try_remote("origin") {
            return ctx;
        }
    }

    // 3. Fall back to first Bitbucket remote
    for name in &remote_names {
        if name == "origin" {
            continue; // already tried
        }
        if let Some(ctx) = try_remote(name) {
            return ctx;
        }
    }

    tracing::debug!(
        count = remote_names.len(),
        "No Bitbucket remote found among remotes"
    );
    GitContext::default()
}

/// Parse git remote URL to extract workspace and repo slug
/// Supports:
/// - HTTPS: https://bitbucket.org/{workspace}/{repo}.git
/// - SSH: git@bitbucket.org:{workspace}/{repo}.git
pub fn parse_git_remote(url: &str) -> Option<(String, String)> {
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

/// Get all git remotes with their URLs
/// Returns a vector of (remote_name, remote_url) tuples
pub fn get_all_remotes() -> Vec<(String, String)> {
    let output = match Command::new("git").args(["remote", "-v"]).output() {
        Ok(output) if output.status.success() => output,
        _ => {
            tracing::debug!("Failed to get git remotes");
            return vec![];
        }
    };

    let output_str = match String::from_utf8(output.stdout) {
        Ok(s) => s,
        Err(_) => {
            tracing::debug!("Invalid UTF-8 in git remotes output");
            return vec![];
        }
    };

    let mut remotes = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in output_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[0].to_string();
            let url = parts[1].to_string();

            // Only add each remote once (git remote -v shows fetch and push)
            let key = (name.clone(), url.clone());
            if seen.insert(key) {
                remotes.push((name, url));
            }
        }
    }

    remotes
}

/// Get the current working directory path
pub fn get_current_directory() -> String {
    env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Detect the current git branch
/// Returns None if in detached HEAD state or not in a git repository
pub fn detect_current_branch() -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8(output.stdout).ok()?.trim().to_string();

    // Empty string indicates detached HEAD
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

/// Get the current commit SHA (useful for detached HEAD state)
pub fn get_current_commit_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let sha = String::from_utf8(output.stdout).ok()?.trim().to_string();

    if sha.is_empty() {
        None
    } else {
        Some(sha)
    }
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
