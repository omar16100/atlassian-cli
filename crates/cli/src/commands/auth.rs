use std::path::Path;

use anyhow::{anyhow, Context, Result};
use atlassiancli_auth::{token_key, CredentialStore};
use atlassiancli_config::Config;
use atlassiancli_output::OutputRenderer;
use clap::{Args, Subcommand};
use serde::Serialize;
use url::Url;

#[derive(Subcommand, Debug, Clone)]
pub enum AuthCommand {
    /// Add or update a profile and store credentials securely
    Login(LoginArgs),
    /// Remove stored credentials (and optionally the profile)
    Logout(LogoutArgs),
    /// List configured profiles
    List,
    /// Show current user information
    Whoami(WhoamiArgs),
    /// Test authentication for a profile
    Test(TestArgs),
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
}

#[derive(Args, Debug, Clone)]
pub struct LoginArgs {
    /// Profile name to create or update.
    #[arg(long)]
    pub profile: String,
    /// Atlassian site base URL (e.g. https://example.atlassian.net).
    #[arg(long)]
    pub base_url: String,
    /// Account email associated with the API token.
    #[arg(long)]
    pub email: String,
    /// API token to store securely (falls back to ATLASSIAN_API_TOKEN env or interactive prompt).
    #[arg(long, env = "ATLASSIAN_API_TOKEN")]
    pub token: Option<String>,
    /// Mark this profile as the default one.
    #[arg(long)]
    pub default: bool,
}

#[derive(Args, Debug, Clone)]
pub struct LogoutArgs {
    /// Profile to remove credentials for.
    #[arg(long)]
    pub profile: String,
    /// Remove the profile from config entirely (not just the stored token).
    #[arg(long)]
    pub remove_profile: bool,
}

pub fn handle(
    command: AuthCommand,
    config: &mut Config,
    config_path: Option<&Path>,
    store: &CredentialStore,
    renderer: &OutputRenderer,
) -> Result<()> {
    match command {
        AuthCommand::Login(args) => login(args, config, config_path, store),
        AuthCommand::Logout(args) => logout(args, config, config_path, store),
        AuthCommand::List => list_profiles(config, store, renderer),
        AuthCommand::Whoami(args) => whoami(args, config, store),
        AuthCommand::Test(args) => test_auth(args, config, store),
    }
}

fn login(
    args: LoginArgs,
    config: &mut Config,
    config_path: Option<&Path>,
    store: &CredentialStore,
) -> Result<()> {
    if args.profile.trim().is_empty() {
        return Err(anyhow!("Profile name cannot be empty"));
    }

    let base_url = Url::parse(&args.base_url)
        .with_context(|| format!("Invalid Atlassian site URL: {}", args.base_url))?;

    let token = match args.token {
        Some(token) if !token.trim().is_empty() => token.trim().to_owned(),
        _ => read_token_from_stdin().context("Failed to read token from prompt")?,
    };
    if token.is_empty() {
        return Err(anyhow!("API token cannot be empty"));
    }

    let profile_entry = config.profiles.entry(args.profile.clone()).or_default();
    profile_entry.base_url = Some(base_url.to_string());
    profile_entry.email = Some(args.email.clone());
    profile_entry.api_token = None; // tokens are stored in the keyring

    if args.default || config.default_profile.is_none() {
        config.default_profile = Some(args.profile.clone());
    }

    let secret_key = token_key(base_url.as_str(), &args.profile);
    store
        .set_secret(&secret_key, &token)
        .context("Failed to store token in keyring")?;

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

fn logout(
    args: LogoutArgs,
    config: &mut Config,
    config_path: Option<&Path>,
    store: &CredentialStore,
) -> Result<()> {
    let profile = config
        .profiles
        .get(&args.profile)
        .ok_or_else(|| anyhow!("Profile '{}' does not exist", args.profile))?;

    let base_url = profile
        .base_url
        .as_deref()
        .ok_or_else(|| anyhow!("Profile '{}' is missing a base_url", args.profile))?;

    let secret_key = token_key(base_url, &args.profile);
    store
        .delete_secret(&secret_key)
        .context("Failed to delete token from keyring")?;

    if args.remove_profile {
        config.profiles.remove(&args.profile);
        if config
            .default_profile
            .as_deref()
            .map(|name| name == args.profile)
            .unwrap_or(false)
        {
            config.default_profile = config.profiles.keys().next().cloned();
        }
    }

    config
        .save(config_path)
        .context("Unable to persist configuration file")?;
    tracing::info!(profile = %args.profile, "Credentials removed");
    Ok(())
}

fn list_profiles(
    config: &Config,
    store: &CredentialStore,
    renderer: &OutputRenderer,
) -> Result<()> {
    #[derive(Serialize)]
    struct Row<'a> {
        name: &'a str,
        base_url: &'a str,
        email: &'a str,
        has_token: bool,
        is_default: bool,
    }

    let mut rows = Vec::new();
    for (name, profile) in &config.profiles {
        let base_url = profile.base_url.as_deref().unwrap_or("");
        let secret_key = token_key(base_url, name);
        let has_token = store.get_secret(&secret_key)?.is_some();
        let row = Row {
            name,
            base_url,
            email: profile.email.as_deref().unwrap_or(""),
            has_token,
            is_default: config
                .default_profile
                .as_deref()
                .map(|default_name| default_name == name)
                .unwrap_or(false),
        };
        rows.push(row);
    }

    if rows.is_empty() {
        tracing::info!("No profiles configured yet. Use `atlassiancli auth login` to add one.");
    }

    renderer.render(&rows)
}

fn read_token_from_stdin() -> Result<String> {
    use std::io::{self, Write};

    print!("Enter API token: ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("Failed to read from stdin")?;

    Ok(line.trim().to_owned())
}

fn whoami(args: WhoamiArgs, config: &Config, store: &CredentialStore) -> Result<()> {
    let (profile_name, profile) = config
        .resolve_profile(args.profile.as_deref())
        .context("No profile found. Use `atlassiancli auth login` to create one.")?;

    let base_url = profile
        .base_url
        .as_deref()
        .context("Profile missing base_url")?;
    let email = profile.email.as_deref().context("Profile missing email")?;

    let secret_key = token_key(base_url, profile_name);
    let token = store
        .get_secret(&secret_key)?
        .context("No API token found. Use `atlassiancli auth login` to authenticate.")?;

    let client = atlassiancli_api::ApiClient::new(base_url)?.with_basic_auth(email, &token);

    let runtime = tokio::runtime::Runtime::new()?;
    let user_data: serde_json::Value = runtime.block_on(async {
        client
            .get("/rest/api/3/myself")
            .await
            .context("Failed to fetch user information from Jira API")
    })?;

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

fn test_auth(args: TestArgs, config: &Config, store: &CredentialStore) -> Result<()> {
    let (profile_name, profile) = config
        .resolve_profile(args.profile.as_deref())
        .context("No profile found. Use `atlassiancli auth login` to create one.")?;

    let base_url = profile
        .base_url
        .as_deref()
        .context("Profile missing base_url")?;
    let email = profile.email.as_deref().context("Profile missing email")?;

    let secret_key = token_key(base_url, profile_name);
    let token = store
        .get_secret(&secret_key)?
        .context("No API token found. Use `atlassiancli auth login` to authenticate.")?;

    println!("Testing authentication for profile '{}'...", profile_name);

    let client = atlassiancli_api::ApiClient::new(base_url)?.with_basic_auth(email, &token);

    let runtime = tokio::runtime::Runtime::new()?;
    let result: Result<serde_json::Value> = runtime.block_on(async {
        client
            .get("/rest/api/3/myself")
            .await
            .context("Authentication test failed")
    });

    match result {
        Ok(_) => {
            println!("✅ Authentication successful!");
            println!("   Profile: {}", profile_name);
            println!("   Email: {}", email);
            println!("   Base URL: {}", base_url);
            Ok(())
        }
        Err(e) => {
            println!("❌ Authentication failed: {}", e);
            Err(e)
        }
    }
}
