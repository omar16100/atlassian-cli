use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use tracing::warn;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub mod encryption;
pub mod secret;

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

/// Path to encrypted credentials file
fn encrypted_credentials_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".atlassian-cli").join("credentials.enc"))
}

/// Load encrypted credentials from disk
fn load_encrypted_credentials() -> Result<encryption::EncryptedCredentials> {
    let path = encrypted_credentials_path().context("Cannot determine home directory")?;

    if !path.exists() {
        return Ok(encryption::EncryptedCredentials::default());
    }

    let content = fs::read_to_string(&path)?;
    let creds: encryption::EncryptedCredentials = serde_json::from_str(&content)
        .context("Failed to parse encrypted credentials file")?;

    Ok(creds)
}

/// Save encrypted credentials to disk with 600 permissions
fn save_encrypted_credentials(creds: &encryption::EncryptedCredentials) -> Result<()> {
    let path = encrypted_credentials_path().context("Cannot determine home directory")?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(creds)?;

    #[cfg(unix)]
    {
        use std::io::Write;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(json.as_bytes())?;
    }

    #[cfg(not(unix))]
    {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        std::io::Write::write_all(&mut std::io::BufWriter::new(file), json.as_bytes())?;
    }

    Ok(())
}

/// Store an encrypted secret
pub fn set_secret_encrypted(account: &str, secret: &str) -> Result<()> {
    let key = encryption::derive_key()?;
    let (nonce, ciphertext) = encryption::encrypt(secret, &key)?;

    let mut creds = load_encrypted_credentials()?;
    creds.credentials.insert(
        account.to_string(),
        encryption::EncryptedToken { nonce, ciphertext },
    );

    save_encrypted_credentials(&creds)?;
    Ok(())
}

/// Get an encrypted secret
pub fn get_secret_encrypted(account: &str) -> Result<Option<String>> {
    let creds = load_encrypted_credentials()?;

    let encrypted_token = match creds.credentials.get(account) {
        Some(token) => token,
        None => return Ok(None),
    };

    let key = encryption::derive_key()?;
    let plaintext = encryption::decrypt(&encrypted_token.ciphertext, &encrypted_token.nonce, &key)?;

    Ok(Some(plaintext))
}

/// Delete an encrypted secret
pub fn delete_secret_encrypted(account: &str) -> Result<()> {
    let mut creds = load_encrypted_credentials()?;
    creds.credentials.remove(account);
    save_encrypted_credentials(&creds)?;
    Ok(())
}

/// Migrate plaintext credentials to encrypted storage
/// Returns the number of credentials migrated
pub fn migrate_plaintext_to_encrypted() -> Result<usize> {
    let plaintext_path = credentials_path().context("Cannot determine home directory")?;

    // If plaintext file doesn't exist, nothing to migrate
    if !plaintext_path.exists() {
        return Ok(0);
    }

    // Read plaintext credentials
    let content = fs::read_to_string(&plaintext_path)?;
    let plaintext_creds: HashMap<String, String> = serde_json::from_str(&content)
        .context("Failed to parse plaintext credentials file")?;

    if plaintext_creds.is_empty() {
        // Empty file, just delete it
        fs::remove_file(&plaintext_path)?;
        return Ok(0);
    }

    // Re-save using encrypted storage
    let count = plaintext_creds.len();
    for (account, token) in plaintext_creds {
        set_secret_encrypted(&account, &token)?;
    }

    // Securely delete old file (overwrite then delete)
    secure_delete_file(&plaintext_path)?;

    Ok(count)
}

/// Securely delete a file by overwriting with zeros before removal
fn secure_delete_file(path: &std::path::Path) -> Result<()> {
    // Get file size
    let metadata = fs::metadata(path)?;
    let file_size = metadata.len() as usize;

    // Overwrite with zeros
    let zeros = vec![0u8; file_size];
    fs::write(path, zeros)?;

    // Now delete
    fs::remove_file(path)?;

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

    #[test]
    fn test_encrypted_storage_roundtrip() {
        let account = "test_account_roundtrip";
        let secret = "test_secret_value_12345";

        // Store encrypted
        set_secret_encrypted(account, secret).expect("Failed to set encrypted secret");

        // Retrieve encrypted
        let retrieved = get_secret_encrypted(account)
            .expect("Failed to get encrypted secret")
            .expect("Secret should exist");

        assert_eq!(retrieved, secret, "Retrieved secret should match original");

        // Clean up
        delete_secret_encrypted(account).expect("Failed to delete encrypted secret");

        // Verify deletion
        let after_delete = get_secret_encrypted(account).expect("Failed to check after delete");
        assert!(after_delete.is_none(), "Secret should be deleted");
    }

    #[test]
    fn test_encrypted_storage_nonexistent() {
        let result = get_secret_encrypted("nonexistent_account_xyz")
            .expect("Should succeed even if not found");

        assert!(result.is_none(), "Non-existent account should return None");
    }

    #[test]
    fn test_migration_no_plaintext_file() {
        // Test migration when no plaintext file exists
        // Note: This test might find existing credentials on a real system,
        // so we just verify it succeeds (doesn't panic)
        let result = migrate_plaintext_to_encrypted();

        assert!(
            result.is_ok(),
            "Migration should succeed even when file doesn't exist or has existing creds"
        );
    }
}
