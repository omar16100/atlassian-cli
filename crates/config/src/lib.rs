use std::{fs, path::Path};

use indexmap::IndexMap;

pub mod paths;
pub use paths::{ConfigDirSource, ConfigPaths, PathEnv};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Represents the full CLI configuration stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: IndexMap<String, Profile>,
}

impl Config {
    /// Load configuration from an explicit path.
    pub fn load_from(path: &Path) -> Result<Self> {
        Self::read(path)
    }

    /// Load configuration from the provided path or the resolved config file.
    pub fn load<P: AsRef<Path>>(path: Option<P>) -> Result<Self> {
        let path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => ConfigPaths::resolve()?.config_file(),
        };
        Self::read(&path)
    }

    fn read(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("Unable to read config file at {}", path.display()))?;

        serde_yaml::from_str(&raw)
            .with_context(|| format!("Malformed YAML in config file {}", path.display()))
    }

    /// Persist the configuration to an explicit path.
    ///
    /// Written owner-only: a profile can carry a plaintext `api_token`, and this
    /// used to land with whatever the umask allowed.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            paths::create_private_dir(parent)?;
        }

        let serialized = serde_yaml::to_string(self)?;
        paths::write_private(path, serialized.as_bytes())
    }

    /// Persist the configuration to the provided path or the resolved config file.
    pub fn save<P: AsRef<Path>>(&self, path: Option<P>) -> Result<()> {
        let path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => ConfigPaths::resolve()?.config_file(),
        };
        self.save_to(&path)
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
}

/// Minimal representation of a profile. Values are optional to support
/// partially configured setups (e.g., when storing tokens in encrypted credential files).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Profile {
    pub base_url: Option<String>,
    pub email: Option<String>,
    pub api_token: Option<String>,
    /// Bitbucket workspace slug (optional, can be inferred from base_url).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Bitbucket token authentication type: "basic" (default) or "bearer".
    /// Bearer is used for repository/workspace/project access tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitbucket_token_type: Option<String>,
    /// OpsGenie API key (optional, separate from Atlassian token).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opsgenie_api_key: Option<String>,
    /// OpsGenie region: "us" or "eu" (defaults to "us").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opsgenie_region: Option<String>,
    /// Bamboo base URL (optional, if different from main base_url).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bamboo_base_url: Option<String>,
    /// Preferred git remote name for Bitbucket auto-detection (default: tries origin, then first Bitbucket remote).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitbucket_remote: Option<String>,
}

/// The Confluence prefix, which belongs to the request path and not to the base.
const WIKI_SUFFIX: &str = "/wiki";

/// A site base URL with a trailing `/wiki` removed.
///
/// The API client appends request paths to the base rather than replacing the
/// base's path, and every Confluence command spells `/wiki` itself while every
/// Jira command spells `/rest/api/3`. So a profile stored as
/// `https://site.atlassian.net/wiki` asked for `/wiki/wiki/api/v2/pages` and
/// `/wiki/rest/api/3/issue/KEY`, and both 404. Writing `/wiki` into the base is
/// an easy mistake to make, because it is how the Confluence REST documentation
/// spells the full URL.
///
/// One `/wiki` segment is removed, not every repetition: a `/wiki/wiki` typo
/// should lose one and stay visibly odd rather than silently becoming the site
/// root. Trailing slashes are all removed first, so `/wiki//` is handled like
/// `/wiki` instead of slipping through the suffix check.
///
/// The comparison ignores case, matching the product detection in
/// `commands/auth.rs`, which lowercases before looking for the same segment.
/// The two disagreeing would mean a `/WIKI` base detected as Confluence but not
/// normalised, which is the doubling all over again.
///
/// Only for Jira, Confluence and JSM. Bamboo is a Server product where a context
/// path in the base is legitimate, and it resolves its own base URL.
pub fn site_base_url(base_url: &str) -> &str {
    let trimmed = base_url.trim_end_matches('/');

    let split = trimmed.len().saturating_sub(WIKI_SUFFIX.len());
    if trimmed.len() >= WIKI_SUFFIX.len()
        && trimmed.is_char_boundary(split)
        && trimmed[split..].eq_ignore_ascii_case(WIKI_SUFFIX)
    {
        return &trimmed[..split];
    }

    trimmed
}

impl Profile {
    /// `base_url` as the site root, for building a Jira, Confluence or JSM client.
    ///
    /// Callers that need to tell the two products apart must read `base_url`
    /// itself: a trailing `/wiki` is the only hint a Confluence-only profile
    /// gives, and this strips it.
    pub fn site_base_url(&self) -> Option<&str> {
        self.base_url.as_deref().map(site_base_url)
    }
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
    fn test_indexmap_preserves_insertion_order() {
        let mut config = Config::default();

        // Insert profiles in specific order
        let profile1 = Profile {
            base_url: Some("https://first.atlassian.net".to_string()),
            ..Default::default()
        };
        config.profiles.insert("first".to_string(), profile1);

        let profile2 = Profile {
            base_url: Some("https://second.atlassian.net".to_string()),
            ..Default::default()
        };
        config.profiles.insert("second".to_string(), profile2);

        let profile3 = Profile {
            base_url: Some("https://third.atlassian.net".to_string()),
            ..Default::default()
        };
        config.profiles.insert("third".to_string(), profile3);

        // Verify iteration order matches insertion order
        let names: Vec<_> = config.profiles.keys().map(|s| s.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn test_resolve_profile_first_available_is_deterministic() {
        // When no default is set, resolve_profile should return the first profile
        // consistently (not randomly based on HashMap iteration)
        let mut config = Config::default();

        // Add multiple profiles
        config.profiles.insert(
            "alpha".to_string(),
            Profile {
                base_url: Some("https://alpha.atlassian.net".to_string()),
                ..Default::default()
            },
        );
        config.profiles.insert(
            "beta".to_string(),
            Profile {
                base_url: Some("https://beta.atlassian.net".to_string()),
                ..Default::default()
            },
        );
        config.profiles.insert(
            "gamma".to_string(),
            Profile {
                base_url: Some("https://gamma.atlassian.net".to_string()),
                ..Default::default()
            },
        );

        // Resolve multiple times and verify we always get the same (first inserted) profile
        for _ in 0..10 {
            let (name, _) = config.resolve_profile(None).unwrap();
            assert_eq!(
                name, "alpha",
                "First profile should always be selected deterministically"
            );
        }
    }

    #[test]
    fn test_shift_remove_preserves_order() {
        let mut config = Config::default();

        config.profiles.insert("a".to_string(), Profile::default());
        config.profiles.insert("b".to_string(), Profile::default());
        config.profiles.insert("c".to_string(), Profile::default());

        // Remove middle element
        config.profiles.shift_remove("b");

        let names: Vec<_> = config.profiles.keys().map(|s| s.as_str()).collect();
        assert_eq!(
            names,
            vec!["a", "c"],
            "Order should be preserved after removal"
        );
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
            ..Default::default()
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

    #[test]
    fn test_profile_bitbucket_token_type_default_none() {
        let profile = Profile::default();
        assert!(profile.bitbucket_token_type.is_none());
    }

    #[test]
    fn test_profile_bitbucket_token_type_bearer() {
        let profile = Profile {
            bitbucket_token_type: Some("bearer".to_string()),
            ..Default::default()
        };
        assert_eq!(profile.bitbucket_token_type.as_deref(), Some("bearer"));
    }

    #[test]
    fn test_profile_bitbucket_token_type_skipped_when_none() {
        // bitbucket_token_type should not appear in YAML when None
        let profile = Profile {
            email: Some("test@example.com".to_string()),
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&profile).unwrap();
        assert!(!yaml.contains("bitbucket_token_type"));
    }

    #[test]
    fn test_profile_bitbucket_token_type_serialized_when_set() {
        let profile = Profile {
            email: Some("test@example.com".to_string()),
            bitbucket_token_type: Some("bearer".to_string()),
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&profile).unwrap();
        assert!(yaml.contains("bitbucket_token_type: bearer"));
    }

    #[test]
    fn test_profile_backwards_compat_missing_token_type() {
        // Old config files without bitbucket_token_type should deserialize fine
        let yaml = "email: test@example.com\nworkspace: myteam\n";
        let profile: Profile = serde_yaml::from_str(yaml).unwrap();
        assert!(profile.bitbucket_token_type.is_none());
        assert_eq!(profile.email.as_deref(), Some("test@example.com"));
        assert_eq!(profile.workspace.as_deref(), Some("myteam"));
    }

    #[test]
    fn test_config_with_bearer_profile_roundtrip() {
        let mut config = Config {
            default_profile: Some("ci".to_string()),
            ..Default::default()
        };

        let profile = Profile {
            workspace: Some("myteam".to_string()),
            bitbucket_token_type: Some("bearer".to_string()),
            ..Default::default()
        };
        config.profiles.insert("ci".to_string(), profile);

        let temp_file = NamedTempFile::new().unwrap();
        config.save(Some(temp_file.path())).unwrap();
        let loaded = Config::load(Some(temp_file.path())).unwrap();

        let ci_profile = loaded.profiles.get("ci").unwrap();
        assert_eq!(ci_profile.bitbucket_token_type.as_deref(), Some("bearer"));
        assert_eq!(ci_profile.workspace.as_deref(), Some("myteam"));
        // No email required for bearer profiles
        assert!(ci_profile.email.is_none());
    }

    #[test]
    fn a_trailing_wiki_is_stripped_from_a_site_base_url() {
        assert_eq!(
            site_base_url("https://site.atlassian.net/wiki"),
            "https://site.atlassian.net"
        );
        assert_eq!(
            site_base_url("https://site.atlassian.net/wiki/"),
            "https://site.atlassian.net"
        );
    }

    #[test]
    fn a_site_root_is_left_alone() {
        assert_eq!(
            site_base_url("https://site.atlassian.net"),
            "https://site.atlassian.net"
        );
        assert_eq!(
            site_base_url("https://site.atlassian.net/"),
            "https://site.atlassian.net"
        );
    }

    /// The reason for `strip_suffix` over `trim_end_matches`: a word merely
    /// ending in "wiki" is not the Confluence prefix.
    #[test]
    fn a_path_that_merely_ends_in_wiki_is_left_alone() {
        assert_eq!(
            site_base_url("https://example.com/mywiki"),
            "https://example.com/mywiki"
        );
        assert_eq!(
            site_base_url("https://wiki.example.com"),
            "https://wiki.example.com"
        );
    }

    /// One segment, not all of them, so a doubled typo stays visible rather
    /// than silently resolving to the site root.
    #[test]
    fn only_one_wiki_segment_is_removed() {
        assert_eq!(
            site_base_url("https://site.atlassian.net/wiki/wiki"),
            "https://site.atlassian.net/wiki"
        );
    }

    /// Every trailing slash goes before the suffix is looked for. Stripping
    /// only one left `/wiki//` unnormalised, so it still doubled while
    /// `auth login` told the user the URL was fine.
    #[test]
    fn repeated_trailing_slashes_do_not_hide_the_suffix() {
        assert_eq!(
            site_base_url("https://site.atlassian.net/wiki//"),
            "https://site.atlassian.net"
        );
        assert_eq!(
            site_base_url("https://site.atlassian.net//"),
            "https://site.atlassian.net"
        );
    }

    /// Product detection lowercases before looking for the same segment. If
    /// this did not, a `/WIKI` base would be called Confluence and left
    /// unnormalised, which is the doubling again.
    #[test]
    fn the_suffix_match_ignores_case() {
        assert_eq!(
            site_base_url("https://site.atlassian.net/WIKI"),
            "https://site.atlassian.net"
        );
        assert_eq!(
            site_base_url("https://site.atlassian.net/Wiki/"),
            "https://site.atlassian.net"
        );
    }

    /// Degenerate input reduces to an empty string rather than panicking on a
    /// slice boundary; `ApiClient::new` then reports it as an unparseable URL.
    #[test]
    fn degenerate_input_does_not_panic() {
        assert_eq!(site_base_url("/wiki"), "");
        assert_eq!(site_base_url(""), "");
        assert_eq!(site_base_url("/"), "");
        assert_eq!(site_base_url("wiki"), "wiki");
        // Multi-byte, to prove the boundary guard is doing something.
        assert_eq!(site_base_url("https://x/päge"), "https://x/päge");
    }

    /// The OAuth gateway form, which the Confluence REST docs spell with the
    /// `/wiki` already attached.
    #[test]
    fn the_gateway_form_reduces_to_its_cloud_id_root() {
        assert_eq!(
            site_base_url("https://api.atlassian.com/ex/confluence/cloud-id/wiki"),
            "https://api.atlassian.com/ex/confluence/cloud-id"
        );
        assert_eq!(
            site_base_url("https://api.atlassian.com/ex/jira/cloud-id"),
            "https://api.atlassian.com/ex/jira/cloud-id"
        );
    }

    /// Bamboo resolves its own base and never calls this, but a context path
    /// must survive if it ever does.
    #[test]
    fn an_unrelated_context_path_survives() {
        assert_eq!(
            site_base_url("https://example.com/bamboo"),
            "https://example.com/bamboo"
        );
    }

    #[test]
    fn the_profile_accessor_follows_the_same_rule() {
        let profile = Profile {
            base_url: Some("https://site.atlassian.net/wiki/".to_string()),
            ..Default::default()
        };
        assert_eq!(profile.site_base_url(), Some("https://site.atlassian.net"));

        assert_eq!(Profile::default().site_base_url(), None);
    }
}
