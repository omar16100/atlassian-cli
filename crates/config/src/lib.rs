use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Represents the full CLI configuration stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
}

impl Config {
    /// Load configuration from the provided path or the default config file.
    pub fn load<P: AsRef<Path>>(path: Option<P>) -> Result<Self> {
        let path = path
            .map(|p| p.as_ref().to_path_buf())
            .unwrap_or_else(Config::default_path);

        if !path.exists() {
            return Ok(Config::default());
        }

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Unable to read config file at {}", path.display()))?;

        serde_yaml::from_str(&raw)
            .with_context(|| format!("Malformed YAML in config file {}", path.display()))
    }

    /// Persist the configuration to disk, creating parent directories if needed.
    pub fn save<P: AsRef<Path>>(&self, path: Option<P>) -> Result<()> {
        let path = path
            .map(|p| p.as_ref().to_path_buf())
            .unwrap_or_else(Config::default_path);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Unable to create config directory {}", parent.display())
            })?;
        }

        let serialized = serde_yaml::to_string(self)?;
        fs::write(&path, serialized)
            .with_context(|| format!("Unable to write config file {}", path.display()))?;

        Ok(())
    }

    /// Convenience helper to retrieve a profile by name.
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    /// Returns either the requested profile or falls back to the default one.
    pub fn resolve_profile<'a>(
        &'a self,
        requested: Option<&'a str>,
    ) -> Option<(&'a str, &'a Profile)> {
        if let Some(name) = requested {
            self.profiles.get(name).map(|profile| (name, profile))
        } else if let Some(default_name) = self.default_profile.as_deref() {
            self.profiles
                .get(default_name)
                .map(|profile| (default_name, profile))
        } else if let Some((name, profile)) = self.profiles.iter().next() {
            Some((name.as_str(), profile))
        } else {
            None
        }
    }

    fn default_path() -> PathBuf {
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(".atlcli");
        path.push("config.yaml");
        path
    }
}

/// Minimal representation of a profile. Values are optional to support
/// partially configured setups (e.g., when storing tokens in the keyring).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Profile {
    pub base_url: Option<String>,
    pub email: Option<String>,
    pub api_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.default_profile.is_none());
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn test_load_missing_file() {
        let result = Config::load(Some("/nonexistent/config.yaml"));
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let mut config = Config {
            default_profile: Some("work".to_string()),
            ..Default::default()
        };

        let profile = Profile {
            base_url: Some("https://test.atlassian.net".to_string()),
            email: Some("test@example.com".to_string()),
            ..Default::default()
        };

        config.profiles.insert("work".to_string(), profile);

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();

        config.save(Some(temp_path)).unwrap();
        let loaded = Config::load(Some(temp_path)).unwrap();

        assert_eq!(loaded.default_profile, Some("work".to_string()));
        assert_eq!(loaded.profiles.len(), 1);

        let work_profile = loaded.profiles.get("work").unwrap();
        assert_eq!(
            work_profile.base_url,
            Some("https://test.atlassian.net".to_string())
        );
        assert_eq!(work_profile.email, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_load_malformed_yaml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "invalid: yaml: [unclosed").unwrap();

        let result = Config::load(Some(temp_file.path()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Malformed YAML"));
    }

    #[test]
    fn test_profile_retrieval() {
        let mut config = Config::default();
        let profile = Profile {
            base_url: Some("https://test.atlassian.net".to_string()),
            ..Default::default()
        };

        config.profiles.insert("test".to_string(), profile);

        assert!(config.profile("test").is_some());
        assert!(config.profile("nonexistent").is_none());
    }

    #[test]
    fn test_resolve_profile_requested() {
        let mut config = Config {
            default_profile: Some("default".to_string()),
            ..Default::default()
        };

        let default_profile = Profile {
            base_url: Some("https://default.atlassian.net".to_string()),
            ..Default::default()
        };
        config
            .profiles
            .insert("default".to_string(), default_profile);

        let work_profile = Profile {
            base_url: Some("https://work.atlassian.net".to_string()),
            ..Default::default()
        };
        config.profiles.insert("work".to_string(), work_profile);

        let (name, profile) = config.resolve_profile(Some("work")).unwrap();
        assert_eq!(name, "work");
        assert_eq!(
            profile.base_url,
            Some("https://work.atlassian.net".to_string())
        );
    }

    #[test]
    fn test_resolve_profile_default() {
        let mut config = Config {
            default_profile: Some("default".to_string()),
            ..Default::default()
        };

        let default_profile = Profile {
            base_url: Some("https://default.atlassian.net".to_string()),
            ..Default::default()
        };
        config
            .profiles
            .insert("default".to_string(), default_profile);

        let (name, profile) = config.resolve_profile(None).unwrap();
        assert_eq!(name, "default");
        assert_eq!(
            profile.base_url,
            Some("https://default.atlassian.net".to_string())
        );
    }

    #[test]
    fn test_resolve_profile_first_available() {
        let mut config = Config::default();

        let profile = Profile {
            base_url: Some("https://only.atlassian.net".to_string()),
            ..Default::default()
        };
        config.profiles.insert("only".to_string(), profile);

        let result = config.resolve_profile(None);
        assert!(result.is_some());
        let (name, profile) = result.unwrap();
        assert_eq!(name, "only");
        assert_eq!(
            profile.base_url,
            Some("https://only.atlassian.net".to_string())
        );
    }

    #[test]
    fn test_resolve_profile_none_available() {
        let config = Config::default();
        assert!(config.resolve_profile(None).is_none());
    }

    #[test]
    fn test_resolve_profile_nonexistent_requested() {
        let mut config = Config::default();

        let profile = Profile {
            base_url: Some("https://test.atlassian.net".to_string()),
            ..Default::default()
        };
        config.profiles.insert("test".to_string(), profile);

        assert!(config.resolve_profile(Some("nonexistent")).is_none());
    }

    #[test]
    fn test_profile_default() {
        let profile = Profile::default();
        assert!(profile.base_url.is_none());
        assert!(profile.email.is_none());
        assert!(profile.api_token.is_none());
    }

    #[test]
    fn test_yaml_serialization() {
        let mut config = Config {
            default_profile: Some("prod".to_string()),
            ..Default::default()
        };

        let profile = Profile {
            base_url: Some("https://prod.atlassian.net".to_string()),
            email: Some("admin@example.com".to_string()),
            api_token: Some("secret-token-123".to_string()),
        };

        config.profiles.insert("prod".to_string(), profile);

        let yaml = serde_yaml::to_string(&config).unwrap();

        assert!(yaml.contains("default_profile: prod"));
        assert!(yaml.contains("https://prod.atlassian.net"));
        assert!(yaml.contains("admin@example.com"));
        assert!(yaml.contains("secret-token-123"));

        let deserialized: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized.default_profile, config.default_profile);
        assert_eq!(deserialized.profiles.len(), 1);
    }
}
