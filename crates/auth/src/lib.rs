use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use tracing::warn;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub mod encryption;
pub mod secret;

/// Bitbucket API base URL.
pub const BITBUCKET_API_URL: &str = "https://api.bitbucket.org";

/// Write a file only its owner can read.
///
/// Via a temporary file in the same directory, then a rename. `OpenOptions::mode`
/// applies only when a file is created, so writing in place would leave an
/// existing 0644 credentials file world-readable. The rename also means a reader
/// never sees a half-written file, which truncate-then-write allows.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(
        ".{}.tmp{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));

    {
        // `create_new`, not `create`: the temp name is derived from the pid, so
        // it is guessable, and opening an existing path would follow a symlink
        // planted there and write the token wherever it points. Failing is the
        // right answer - a leftover means a crash, and the retry below clears it
        // only after confirming it is a plain file we own.
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);

        let mut file = match options.open(&tmp) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = fs::symlink_metadata(&tmp)
                    .map(|m| m.is_file())
                    .unwrap_or(false);
                if !stale {
                    return Err(anyhow::anyhow!(
                        "Refusing to write {}: it exists and is not a regular file",
                        tmp.display()
                    ));
                }
                fs::remove_file(&tmp)
                    .with_context(|| format!("Unable to clear stale {}", tmp.display()))?;
                options
                    .open(&tmp)
                    .with_context(|| format!("Unable to write {}", tmp.display()))?
            }
            Err(e) => {
                return Err(anyhow::Error::new(e))
                    .with_context(|| format!("Unable to write {}", tmp.display()))
            }
        };
        file.write_all(bytes)
            .with_context(|| format!("Unable to write {}", tmp.display()))?;
        file.sync_all().ok();
    }

    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        anyhow::anyhow!("Unable to write {}: {}", path.display(), e)
    })?;

    Ok(())
}

/// Create a directory only its owner can enter.
fn create_private_dir(dir: &Path) -> Result<()> {
    // Only a directory we create is ours to set a mode on; see the matching
    // helper in atlassian-cli-config for why tightening an existing one is wrong.
    if dir.is_dir() {
        return Ok(());
    }

    fs::create_dir_all(dir)
        .with_context(|| format!("Unable to create directory {}", dir.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("Unable to restrict permissions on {}", dir.display()))?;
    }

    Ok(())
}

/// Helper to construct a key for profile secrets.
pub fn token_key(profile: &str) -> String {
    profile.to_string()
}

/// Helper to construct a key for Bitbucket profile secrets.
pub fn bitbucket_token_key(profile: &str) -> String {
    format!("{}_bitbucket", profile)
}

/// Where the CLI keeps its credentials.
///
/// The directory is supplied by the caller rather than derived here. That is
/// what lets `$ATLASSIAN_CLI_CONFIG_DIR` move it, and it is what lets the tests
/// run against a temporary directory instead of the developer's real one.
#[derive(Debug, Clone)]
pub struct CredentialStore {
    dir: PathBuf,
}

impl CredentialStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The pre-encryption credentials file. Still read so an old install keeps
    /// working until `migrate_plaintext_to_encrypted` runs.
    pub fn credentials_path(&self) -> PathBuf {
        self.dir.join("credentials")
    }

    pub fn encrypted_path(&self) -> PathBuf {
        self.dir.join("credentials.enc")
    }

    fn read_plaintext(&self) -> Result<HashMap<String, String>> {
        let path = self.credentials_path();
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Unable to read {}", path.display()))?;
        Ok(serde_json::from_str(&content).unwrap_or_else(|e| {
            warn!("Failed to parse credentials file: {}", e);
            HashMap::new()
        }))
    }

    fn write_plaintext(&self, creds: &HashMap<String, String>) -> Result<()> {
        create_private_dir(&self.dir)?;
        let json = serde_json::to_string_pretty(creds)?;
        write_private(&self.credentials_path(), json.as_bytes())
    }

    /// Store a secret in the plaintext credentials file.
    pub fn set(&self, account: &str, secret: &str) -> Result<()> {
        let mut creds = self.read_plaintext()?;
        creds.insert(account.to_string(), secret.to_string());
        self.write_plaintext(&creds)
    }

    /// Read a secret from the plaintext credentials file.
    pub fn get(&self, account: &str) -> Result<Option<String>> {
        let path = self.credentials_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Unable to read {}", path.display()))?;
        let creds: HashMap<String, String> = serde_json::from_str(&content)?;
        Ok(creds.get(account).cloned())
    }

    /// Remove a secret from the plaintext credentials file.
    pub fn delete(&self, account: &str) -> Result<()> {
        if !self.credentials_path().exists() {
            return Ok(());
        }
        let mut creds = self.read_plaintext()?;
        creds.remove(account);
        self.write_plaintext(&creds)
    }

    fn load_encrypted(&self) -> Result<encryption::EncryptedCredentials> {
        let path = self.encrypted_path();
        if !path.exists() {
            return Ok(encryption::EncryptedCredentials::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Unable to read {}", path.display()))?;
        serde_json::from_str(&content).context("Failed to parse encrypted credentials file")
    }

    fn save_encrypted(&self, creds: &encryption::EncryptedCredentials) -> Result<()> {
        create_private_dir(&self.dir)?;
        let json = serde_json::to_string_pretty(creds)?;
        write_private(&self.encrypted_path(), json.as_bytes())
    }

    /// Store an encrypted secret.
    pub fn set_encrypted(&self, account: &str, secret: &str) -> Result<()> {
        let key = encryption::derive_key()?;
        let (nonce, ciphertext) = encryption::encrypt(secret, &key)?;

        let mut creds = self.load_encrypted()?;
        creds.credentials.insert(
            account.to_string(),
            encryption::EncryptedToken { nonce, ciphertext },
        );

        self.save_encrypted(&creds)
    }

    /// Read an encrypted secret.
    pub fn get_encrypted(&self, account: &str) -> Result<Option<String>> {
        let creds = self.load_encrypted()?;

        let encrypted_token = match creds.credentials.get(account) {
            Some(token) => token,
            None => return Ok(None),
        };

        let key = encryption::derive_key()?;
        let plaintext =
            encryption::decrypt(&encrypted_token.ciphertext, &encrypted_token.nonce, &key)?;

        Ok(Some(plaintext))
    }

    /// Remove an encrypted secret.
    pub fn delete_encrypted(&self, account: &str) -> Result<()> {
        let mut creds = self.load_encrypted()?;
        creds.credentials.remove(account);
        self.save_encrypted(&creds)
    }

    /// Re-store any plaintext credentials as encrypted ones, then securely
    /// delete the plaintext file. Returns how many were migrated.
    pub fn migrate_plaintext_to_encrypted(&self) -> Result<usize> {
        let plaintext_path = self.credentials_path();
        if !plaintext_path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&plaintext_path)
            .with_context(|| format!("Unable to read {}", plaintext_path.display()))?;
        let plaintext_creds: HashMap<String, String> =
            serde_json::from_str(&content).context("Failed to parse plaintext credentials file")?;

        if plaintext_creds.is_empty() {
            fs::remove_file(&plaintext_path)?;
            return Ok(0);
        }

        let count = plaintext_creds.len();
        for (account, token) in plaintext_creds {
            self.set_encrypted(&account, &token)?;
        }

        secure_delete_file(&plaintext_path)?;

        Ok(count)
    }
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
    use tempfile::TempDir;

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

    /// A store rooted in a temporary directory.
    ///
    /// These tests used to run against the real `~/.atlassian-cli`:
    /// `test_encrypted_storage_roundtrip` read-modify-wrote a developer's actual
    /// `credentials.enc`, and the migration test called a function that
    /// securely deletes a real plaintext `credentials` file. That is the reason
    /// `CredentialStore` takes its directory rather than deriving one.
    fn store() -> (TempDir, CredentialStore) {
        let dir = TempDir::new().expect("failed to create a temp dir");
        let store = CredentialStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn test_encrypted_storage_roundtrip() {
        let (_dir, store) = store();
        let account = "test_account_roundtrip";
        let secret = "test_secret_value_12345";

        store
            .set_encrypted(account, secret)
            .expect("Failed to set encrypted secret");

        let retrieved = store
            .get_encrypted(account)
            .expect("Failed to get encrypted secret")
            .expect("Secret should exist");
        assert_eq!(retrieved, secret, "Retrieved secret should match original");

        store
            .delete_encrypted(account)
            .expect("Failed to delete encrypted secret");

        let after_delete = store
            .get_encrypted(account)
            .expect("Failed to check after delete");
        assert!(after_delete.is_none(), "Secret should be deleted");
    }

    #[test]
    fn test_encrypted_storage_nonexistent() {
        let (_dir, store) = store();
        let result = store
            .get_encrypted("nonexistent_account_xyz")
            .expect("Should succeed even if not found");

        assert!(result.is_none(), "Non-existent account should return None");
    }

    /// Nothing to migrate is not an error. Previously this ran against the real
    /// home directory, so on a machine with a plaintext credentials file it
    /// would delete it.
    #[test]
    fn test_migration_no_plaintext_file() {
        let (_dir, store) = store();
        assert_eq!(
            store
                .migrate_plaintext_to_encrypted()
                .expect("Migration should succeed when there is no file"),
            0
        );
    }

    #[test]
    fn test_migration_moves_plaintext_into_encrypted_storage() {
        let (_dir, store) = store();
        let mut plaintext = HashMap::new();
        plaintext.insert("work".to_string(), "token-a".to_string());
        plaintext.insert("personal".to_string(), "token-b".to_string());
        store
            .write_plaintext(&plaintext)
            .expect("failed to seed plaintext credentials");

        let count = store
            .migrate_plaintext_to_encrypted()
            .expect("migration should succeed");

        assert_eq!(count, 2);
        assert!(
            !store.credentials_path().exists(),
            "the plaintext file should be gone once migrated"
        );
        assert_eq!(
            store.get_encrypted("work").unwrap().as_deref(),
            Some("token-a")
        );
        assert_eq!(
            store.get_encrypted("personal").unwrap().as_deref(),
            Some("token-b")
        );
    }

    #[test]
    fn test_plaintext_roundtrip_and_delete() {
        let (_dir, store) = store();
        assert!(store.get("absent").unwrap().is_none());

        store.set("work", "token").unwrap();
        assert_eq!(store.get("work").unwrap().as_deref(), Some("token"));

        store.delete("work").unwrap();
        assert!(store.get("work").unwrap().is_none());
    }

    /// The store creates its own directory rather than requiring the caller to.
    #[test]
    fn test_writing_creates_a_private_directory() {
        let parent = TempDir::new().unwrap();
        let dir = parent.path().join("nested").join("config");
        let store = CredentialStore::new(&dir);

        store.set_encrypted("work", "token").unwrap();

        assert!(dir.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "config directory should be owner-only");
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_credentials_are_owner_only_even_when_overwriting() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, store) = store();
        store.set_encrypted("work", "token").unwrap();

        // Loosen it the way an older version of this code could have left it,
        // then write again. OpenOptions::mode only applies at creation, so an
        // in-place write would leave this world-readable.
        fs::set_permissions(store.encrypted_path(), fs::Permissions::from_mode(0o644)).unwrap();
        store.set_encrypted("work", "token2").unwrap();

        let mode = fs::metadata(store.encrypted_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "credentials must not stay world-readable");
        assert_eq!(
            store.get_encrypted("work").unwrap().as_deref(),
            Some("token2")
        );
    }
}
