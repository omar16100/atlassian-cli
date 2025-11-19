use anyhow::{Context, Result};
use keyring::{Entry, Error as KeyringError};

/// Wrapper around the system keyring to store secrets per profile.
pub struct CredentialStore {
    service_name: String,
}

impl CredentialStore {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }

    pub fn set_secret(&self, account: &str, secret: &str) -> Result<()> {
        Entry::new(&self.service_name, account)
            .with_context(|| format!("Unable to access keyring entry {account}"))?
            .set_password(secret)
            .with_context(|| format!("Unable to store secret for {account}"))
    }

    pub fn get_secret(&self, account: &str) -> Result<Option<String>> {
        let entry = Entry::new(&self.service_name, account)
            .with_context(|| format!("Unable to access keyring entry {account}"))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(err) => Err(err).with_context(|| format!("Unable to read secret for {account}")),
        }
    }

    pub fn delete_secret(&self, account: &str) -> Result<()> {
        let entry = Entry::new(&self.service_name, account)
            .with_context(|| format!("Unable to access keyring entry {account}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(err) => Err(err).with_context(|| format!("Unable to delete secret for {account}")),
        }
    }
}

/// Helper to construct a key for per-instance secrets.
pub fn token_key(instance: &str, profile: &str) -> String {
    format!("{instance}:{profile}")
}
