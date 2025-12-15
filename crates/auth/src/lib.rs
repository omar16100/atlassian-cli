use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use tracing::warn;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Bitbucket API base URL.
pub const BITBUCKET_API_URL: &str = "https://api.bitbucket.org";

/// Helper to construct a key for profile secrets.
pub fn token_key(profile: &str) -> String {
    profile.to_string()
}

/// Helper to construct a key for Bitbucket profile secrets.
pub fn bitbucket_token_key(profile: &str) -> String {
    format!("{}_bitbucket", profile)
}

fn credentials_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".atlassian-cli").join("credentials"))
}

/// Store a secret in the credentials file with 600 permissions.
pub fn set_secret(account: &str, secret: &str) -> Result<()> {
    let path = credentials_path().context("Cannot determine home directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut creds: HashMap<String, String> = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content).unwrap_or_else(|e| {
            warn!("Failed to parse credentials file: {}", e);
            HashMap::new()
        })
    } else {
        HashMap::new()
    };

    creds.insert(account.to_string(), secret.to_string());

    #[cfg(unix)]
    {
        use std::io::Write;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        let json = serde_json::to_string_pretty(&creds)?;
        file.write_all(json.as_bytes())?;
    }

    #[cfg(not(unix))]
    {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        serde_json::to_writer_pretty(file, &creds)?;
    }

    Ok(())
}

/// Get a secret from the credentials file.
pub fn get_secret(account: &str) -> Result<Option<String>> {
    let path = credentials_path().context("Cannot determine home directory")?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    let creds: HashMap<String, String> = serde_json::from_str(&content)?;
    Ok(creds.get(account).cloned())
}

/// Delete a secret from the credentials file.
pub fn delete_secret(account: &str) -> Result<()> {
    let path = credentials_path().context("Cannot determine home directory")?;
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&path)?;
    let mut creds: HashMap<String, String> = serde_json::from_str(&content).unwrap_or_else(|e| {
        warn!("Failed to parse credentials file: {}", e);
        HashMap::new()
    });
    creds.remove(account);

    #[cfg(unix)]
    {
        use std::io::Write;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        let json = serde_json::to_string_pretty(&creds)?;
        file.write_all(json.as_bytes())?;
    }

    #[cfg(not(unix))]
    {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        serde_json::to_writer_pretty(file, &creds)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_key() {
        assert_eq!(token_key("work"), "work");
        assert_eq!(token_key("my-profile"), "my-profile");
    }

    #[test]
    fn test_bitbucket_token_key() {
        assert_eq!(bitbucket_token_key("work"), "work_bitbucket");
        assert_eq!(bitbucket_token_key("my-profile"), "my-profile_bitbucket");
    }

    #[test]
    fn test_bitbucket_api_url() {
        assert_eq!(BITBUCKET_API_URL, "https://api.bitbucket.org");
    }
}
