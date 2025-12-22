use std::path::Path;

use anyhow::{anyhow, Context, Result};
use atlassian_cli_auth::{bitbucket_token_key, token_key, BITBUCKET_API_URL};
use atlassian_cli_config::Config;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};
use serde::Serialize;
use tracing::debug;
use url::Url;

/// Multi-tier token lookup: env var → encrypted credentials → plaintext credentials (migration fallback)
pub fn get_token(profile_name: &str) -> Option<String> {
    // 1. Check profile-specific env var: ATLASSIAN_CLI_TOKEN_{PROFILE}
    let profile_env_var = format!("ATLASSIAN_CLI_TOKEN_{}", profile_name.to_uppercase());
    std::env::var(&profile_env_var)
        .ok()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            // 2. Check generic env var: ATLASSIAN_API_TOKEN
            std::env::var("ATLASSIAN_API_TOKEN")
                .ok()
                .filter(|t| !t.trim().is_empty())
        })
        .or_else(|| {
            // 3. Try encrypted credentials file
            let secret_key = token_key(profile_name);
            atlassian_cli_auth::get_secret_encrypted(&secret_key)
                .ok()
                .flatten()
                .or_else(|| {
                    // 4. Fallback to plaintext credentials (for migration)
                    atlassian_cli_auth::get_secret(&secret_key).ok().flatten()
                })
        })
}

/// Multi-tier Bitbucket token lookup: env var → credentials file
/// Note: Does NOT fall back to general token. Caller should handle fallback if needed.
pub fn get_bitbucket_token(profile_name: &str) -> Option<String> {
    // 1. Profile-specific Bitbucket env var
    let profile_env_var = format!(
        "ATLASSIAN_CLI_BITBUCKET_TOKEN_{}",
        profile_name.to_uppercase()
    );
    std::env::var(&profile_env_var)
        .ok()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            // 2. Generic Bitbucket env var
            std::env::var("ATLASSIAN_BITBUCKET_TOKEN")
                .ok()
                .filter(|t| !t.trim().is_empty())
        })
        .or_else(|| {
            // 3. BITBUCKET_TOKEN env var
            std::env::var("BITBUCKET_TOKEN")
                .ok()
                .filter(|t| !t.trim().is_empty())
        })
        .or_else(|| {
            // 4. Try encrypted credentials file
            let secret_key = bitbucket_token_key(profile_name);
            atlassian_cli_auth::get_secret_encrypted(&secret_key)
                .ok()
                .flatten()
                .or_else(|| {
                    // 5. Fallback to plaintext credentials (for migration)
                    atlassian_cli_auth::get_secret(&secret_key).ok().flatten()
                })
        })
}

#[derive(Subcommand, Debug, Clone)]
pub enum AuthCommand {
    /// Add or update a profile and store credentials securely
    Login(LoginArgs),
    /// Remove stored credentials (and optionally the profile)
    Logout(LogoutArgs),
    /// List configured profiles
    List(ListArgs),
    /// Show current user information
    Whoami(WhoamiArgs),
    /// Test authentication for a profile
    Test(TestArgs),
}

#[derive(Args, Debug, Clone)]
pub struct ListArgs {
    /// Show all profiles, including those without active tokens.
    #[arg(long)]
    pub all: bool,
}

#[derive(Args, Debug, Clone)]
pub struct WhoamiArgs {
    /// Profile to use (defaults to default profile)
    #[arg(long)]
    pub profile: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct TestArgs {
    /// Profile to test (defaults to default profile)
    #[arg(long)]
    pub profile: Option<String>,
    /// Test Bitbucket authentication instead of Jira/Confluence.
    #[arg(long)]
    pub bitbucket: bool,
}

#[derive(Args, Debug, Clone)]
pub struct LoginArgs {
    /// Profile name to create or update.
    #[arg(long)]
    pub profile: String,
    /// Atlassian site base URL (e.g. https://example.atlassian.net). Not required for --bitbucket.
    #[arg(long, required_unless_present = "bitbucket")]
    pub base_url: Option<String>,
    /// Account email associated with the API token.
    #[arg(long)]
    pub email: String,
    /// API token to store securely (falls back to ATLASSIAN_API_TOKEN env or interactive prompt).
    #[arg(long, env = "ATLASSIAN_API_TOKEN")]
    pub token: Option<String>,
    /// Mark this profile as the default one.
    #[arg(long)]
    pub default: bool,
    /// Login for Bitbucket (uses api.bitbucket.org, no base_url required).
    #[arg(long)]
    pub bitbucket: bool,
    /// Bitbucket workspace slug (optional, for --bitbucket mode).
    #[arg(long, requires = "bitbucket")]
    pub workspace: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct LogoutArgs {
    /// Profile to remove credentials for.
    #[arg(long)]
    pub profile: String,
    /// Remove the profile from config entirely (not just the stored token).
    #[arg(long)]
    pub remove_profile: bool,
    /// Only remove Bitbucket token (keep Jira/Confluence token).
    #[arg(long)]
    pub bitbucket: bool,
}

pub async fn handle(
    command: AuthCommand,
    config: &mut Config,
    config_path: Option<&Path>,
    renderer: &OutputRenderer,
) -> Result<()> {
    match command {
        AuthCommand::Login(args) => login(args, config, config_path),
        AuthCommand::Logout(args) => logout(args, config, config_path),
        AuthCommand::List(args) => list_profiles(args, config, renderer),
        AuthCommand::Whoami(args) => whoami(args, config).await,
        AuthCommand::Test(args) => test_auth(args, config).await,
    }
}

fn login(args: LoginArgs, config: &mut Config, config_path: Option<&Path>) -> Result<()> {
    // Migrate any existing plaintext credentials to encrypted storage
    if let Ok(count) = atlassian_cli_auth::migrate_plaintext_to_encrypted() {
        if count > 0 {
            tracing::info!(
                credentials_migrated = count,
                "Migrated plaintext credentials to encrypted storage"
            );
        }
    }

    if args.profile.trim().is_empty() {
        return Err(anyhow!("Profile name cannot be empty"));
    }

    let token = match &args.token {
        Some(token) if !token.trim().is_empty() => token.trim().to_owned(),
        _ => read_token_from_stdin(args.bitbucket).context("Failed to read token from prompt")?,
    };
    if token.is_empty() {
        return Err(anyhow!("API token cannot be empty"));
    }

    if args.bitbucket {
        login_bitbucket(&args, &token, config, config_path)
    } else {
        login_jira_confluence(&args, &token, config, config_path)
    }
}

fn login_jira_confluence(
    args: &LoginArgs,
    token: &str,
    config: &mut Config,
    config_path: Option<&Path>,
) -> Result<()> {
    let base_url_str = args
        .base_url
        .as_ref()
        .ok_or_else(|| anyhow!("--base-url is required for Jira/Confluence login"))?;

    let base_url = Url::parse(base_url_str)
        .with_context(|| format!("Invalid Atlassian site URL: {}", base_url_str))?;

    // Enforce HTTPS for security
    if base_url.scheme() != "https" {
        return Err(anyhow!(
            "Only HTTPS URLs are allowed for security. Got: {}://",
            base_url.scheme()
        ));
    }

    let profile_entry = config.profiles.entry(args.profile.clone()).or_default();
    profile_entry.base_url = Some(base_url.to_string());
    profile_entry.email = Some(args.email.clone());
    profile_entry.api_token = None;

    if args.default || config.default_profile.is_none() {
        config.default_profile = Some(args.profile.clone());
    }

    let secret_key = token_key(&args.profile);
    atlassian_cli_auth::set_secret_encrypted(&secret_key, token)
        .context("Failed to store token in encrypted credentials file")?;

    config
        .save(config_path)
        .context("Unable to persist configuration file")?;

    tracing::info!(
        profile = %args.profile,
        base_url = %base_url,
        "Profile saved and token stored securely"
    );
    Ok(())
}

fn login_bitbucket(
    args: &LoginArgs,
    token: &str,
    config: &mut Config,
    config_path: Option<&Path>,
) -> Result<()> {
    let profile_entry = config.profiles.entry(args.profile.clone()).or_default();

    // Update workspace if provided
    if args.workspace.is_some() {
        profile_entry.workspace = args.workspace.clone();
    }

    // Ensure email is set
    profile_entry.email = Some(args.email.clone());

    if args.default || config.default_profile.is_none() {
        config.default_profile = Some(args.profile.clone());
    }

    // Store Bitbucket token with _bitbucket suffix
    let secret_key = bitbucket_token_key(&args.profile);
    atlassian_cli_auth::set_secret_encrypted(&secret_key, token)
        .context("Failed to store Bitbucket token in encrypted credentials file")?;

    config
        .save(config_path)
        .context("Unable to persist configuration file")?;

    tracing::info!(
        profile = %args.profile,
        workspace = ?args.workspace,
        "Bitbucket credentials saved successfully"
    );
    Ok(())
}

fn logout(args: LogoutArgs, config: &mut Config, config_path: Option<&Path>) -> Result<()> {
    let _profile = config
        .profiles
        .get(&args.profile)
        .ok_or_else(|| anyhow!("Profile '{}' does not exist", args.profile))?;

    if args.bitbucket {
        // Only remove Bitbucket token
        let secret_key = bitbucket_token_key(&args.profile);
        // Try encrypted first, then plaintext (for migration cleanup)
        if let Err(e) = atlassian_cli_auth::delete_secret_encrypted(&secret_key) {
            debug!("No encrypted Bitbucket token to delete: {e}");
            if let Err(e) = atlassian_cli_auth::delete_secret(&secret_key) {
                debug!("No plaintext Bitbucket token to delete: {e}");
            }
        }
        tracing::info!(profile = %args.profile, "Bitbucket credentials removed");
    } else {
        // Remove both Jira and Bitbucket tokens
        let secret_key = token_key(&args.profile);
        // Try encrypted first, then plaintext (for migration cleanup)
        if let Err(e) = atlassian_cli_auth::delete_secret_encrypted(&secret_key) {
            debug!("No encrypted Jira token to delete: {e}");
            if let Err(e) = atlassian_cli_auth::delete_secret(&secret_key) {
                debug!("No plaintext Jira token to delete: {e}");
            }
        }

        let bb_secret_key = bitbucket_token_key(&args.profile);
        // Try encrypted first, then plaintext (for migration cleanup)
        if let Err(e) = atlassian_cli_auth::delete_secret_encrypted(&bb_secret_key) {
            debug!("No encrypted Bitbucket token to delete: {e}");
            if let Err(e) = atlassian_cli_auth::delete_secret(&bb_secret_key) {
                debug!("No plaintext Bitbucket token to delete: {e}");
            }
        }
        tracing::info!(profile = %args.profile, "Credentials removed");
    }

    if args.remove_profile {
        config.profiles.shift_remove(&args.profile);
        if config
            .default_profile
            .as_deref()
            .map(|name| name == args.profile)
            .unwrap_or(false)
        {
            config.default_profile = None;  // Force explicit re-selection
        }
    }

    config
        .save(config_path)
        .context("Unable to persist configuration file")?;
    Ok(())
}

fn list_profiles(args: ListArgs, config: &Config, renderer: &OutputRenderer) -> Result<()> {
    #[derive(Serialize)]
    struct Row<'a> {
        name: &'a str,
        base_url: &'a str,
        email: &'a str,
        has_jira_token: bool,
        has_bitbucket_token: bool,
        workspace: &'a str,
        is_default: bool,
    }

    let mut rows = Vec::new();

    // Collect and sort profile names for deterministic, alphabetical display
    let mut profile_names: Vec<_> = config.profiles.keys().collect();
    profile_names.sort();

    for name in profile_names {
        let profile = &config.profiles[name];
        let has_jira_token = get_token(name).is_some();
        let has_bitbucket_token = get_bitbucket_token(name).is_some();

        // Only show profiles with at least one active token, unless --all is specified
        if !args.all && !has_jira_token && !has_bitbucket_token {
            continue;
        }

        let row = Row {
            name,
            base_url: profile.base_url.as_deref().unwrap_or(""),
            email: profile.email.as_deref().unwrap_or(""),
            has_jira_token,
            has_bitbucket_token,
            workspace: profile.workspace.as_deref().unwrap_or(""),
            is_default: config
                .default_profile
                .as_deref()
                .map(|default_name| default_name == name)
                .unwrap_or(false),
        };
        rows.push(row);
    }

    if rows.is_empty() {
        if args.all {
            tracing::info!("No profiles configured. Use `atlassian-cli auth login` to add one.");
        } else {
            tracing::info!("No profiles with active credentials. Use `atlassian-cli auth login` to add one, or `auth list --all` to see all profiles.");
        }
    }

    renderer.render(&rows)
}

fn read_token_from_stdin(is_bitbucket: bool) -> Result<String> {
    use std::io::{self, Write};

    if is_bitbucket {
        println!(
            "Create an app password at: https://bitbucket.org/account/settings/app-passwords/"
        );
        println!("Required scopes: Account (read), Repositories (read/write), Pull requests (read/write)");
    } else {
        println!(
            "You can get the API token from: https://id.atlassian.com/manage-profile/security/api-tokens"
        );
    }
    print!("Enter API token: ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let token = rpassword::read_password().context("Failed to read token")?;
    Ok(token.trim().to_owned())
}

async fn whoami(args: WhoamiArgs, config: &Config) -> Result<()> {
    let (profile_name, profile) = config
        .resolve_profile(args.profile.as_deref())
        .context("No profile found. Use `atlassian-cli auth login` to create one.")?;

    let base_url = profile
        .base_url
        .as_deref()
        .context("Profile missing base_url")?;
    let email = profile.email.as_deref().context("Profile missing email")?;

    let token = get_token(profile_name).ok_or_else(|| {
        anyhow!(
            "No token found for profile '{profile_name}'. Set ATLASSIAN_CLI_TOKEN_{} env var or run `atlassian-cli auth login`",
            profile_name.to_uppercase()
        )
    })?;

    let client = atlassian_cli_api::ApiClient::new(base_url)?.with_basic_auth(email, &token);

    let user_data: serde_json::Value = client
        .get("/rest/api/3/myself")
        .await
        .context("Failed to fetch user information from Jira API")?;

    println!("Profile: {}", profile_name);
    println!(
        "Display Name: {}",
        user_data["displayName"].as_str().unwrap_or("Unknown")
    );
    println!(
        "Email: {}",
        user_data["emailAddress"].as_str().unwrap_or("Unknown")
    );
    println!(
        "Account ID: {}",
        user_data["accountId"].as_str().unwrap_or("Unknown")
    );
    println!("Active: {}", user_data["active"].as_bool().unwrap_or(false));

    Ok(())
}

async fn test_auth(args: TestArgs, config: &Config) -> Result<()> {
    let (profile_name, profile) = config
        .resolve_profile(args.profile.as_deref())
        .context("No profile found. Use `atlassian-cli auth login` to create one.")?;

    let email = profile.email.as_deref().context("Profile missing email")?;

    if args.bitbucket {
        test_bitbucket_auth(profile_name, email).await
    } else {
        let base_url = profile
            .base_url
            .as_deref()
            .context("Profile missing base_url. For Bitbucket, use --bitbucket flag.")?;
        test_jira_auth(profile_name, email, base_url).await
    }
}

async fn test_jira_auth(profile_name: &str, email: &str, base_url: &str) -> Result<()> {
    let token = get_token(profile_name).ok_or_else(|| {
        anyhow!("No Jira token found for profile '{profile_name}'. Run `atlassian-cli auth login`")
    })?;

    println!(
        "Testing Jira authentication for profile '{}'...",
        profile_name
    );

    let client = atlassian_cli_api::ApiClient::new(base_url)?.with_basic_auth(email, &token);

    let result: Result<serde_json::Value> = client
        .get("/rest/api/3/myself")
        .await
        .context("Jira authentication test failed");

    match result {
        Ok(_) => {
            println!("Authentication successful!");
            println!("   Profile: {}", profile_name);
            println!("   Email: {}", email);
            println!("   Base URL: {}", base_url);
            Ok(())
        }
        Err(e) => {
            println!("Authentication failed: {}", e);
            Err(e)
        }
    }
}

async fn test_bitbucket_auth(profile_name: &str, email: &str) -> Result<()> {
    let token = get_bitbucket_token(profile_name).ok_or_else(|| {
        anyhow!(
            "No Bitbucket token found for profile '{profile_name}'. \
            Set BITBUCKET_TOKEN or ATLASSIAN_CLI_BITBUCKET_TOKEN_{} env var, \
            or run `atlassian-cli auth login --bitbucket`",
            profile_name.to_uppercase()
        )
    })?;

    println!(
        "Testing Bitbucket authentication for profile '{}'...",
        profile_name
    );

    let client =
        atlassian_cli_api::ApiClient::new(BITBUCKET_API_URL)?.with_basic_auth(email, &token);

    let result: Result<serde_json::Value> = client
        .get("/2.0/user")
        .await
        .context("Bitbucket authentication test failed");

    match result {
        Ok(user_data) => {
            println!("Bitbucket authentication successful!");
            println!("   Profile: {}", profile_name);
            println!(
                "   Username: {}",
                user_data["username"].as_str().unwrap_or("Unknown")
            );
            println!(
                "   Display Name: {}",
                user_data["display_name"].as_str().unwrap_or("Unknown")
            );
            Ok(())
        }
        Err(e) => {
            println!("Bitbucket authentication failed: {}", e);
            Err(e)
        }
    }
}
