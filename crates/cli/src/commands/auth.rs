use std::io::{IsTerminal, Write};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use atlassian_cli_auth::{bitbucket_token_key, token_key, BITBUCKET_API_URL};
use atlassian_cli_config::Config;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};
use serde::Serialize;
use tracing::debug;
use url::Url;

/// Check if a profile uses Bearer auth for Bitbucket.
pub fn is_bitbucket_bearer(config: &Config, profile_name: &str) -> bool {
    config
        .profiles
        .get(profile_name)
        .and_then(|p| p.bitbucket_token_type.as_deref())
        .map(|t| t == "bearer")
        .unwrap_or(false)
}

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
    /// Show authentication status for all services
    Status(StatusArgs),
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
pub struct StatusArgs {
    /// Profile to check (defaults to default profile)
    #[arg(long)]
    pub profile: Option<String>,
    /// Only show configured services (hide N/A entries)
    #[arg(long)]
    pub configured_only: bool,
}

#[derive(Args, Debug, Clone)]
#[command(after_help = "EXAMPLES:\n  \
    Jira/Confluence: atlassian-cli auth login --profile work --base-url https://example.atlassian.net --email user@example.com --token <TOKEN>\n  \
    Bitbucket (API token):    atlassian-cli auth login --profile work --bitbucket --email user@example.com --token <TOKEN>\n  \
    Bitbucket (access token): atlassian-cli auth login --profile work --bitbucket --bearer --token <TOKEN>\n\n\
    Note: App passwords are deprecated. Use Bitbucket API tokens (Basic auth) or access tokens (Bearer auth).\n  \
    Create API tokens at: https://id.atlassian.com/manage-profile/security/api-tokens -> Select 'Bitbucket'")]
pub struct LoginArgs {
    /// Profile name to create or update. Prompted for if omitted.
    #[arg(long)]
    pub profile: Option<String>,
    /// Atlassian site base URL (e.g. https://example.atlassian.net). Prompted for
    /// if omitted. Not required for --bitbucket.
    #[arg(long)]
    pub base_url: Option<String>,
    /// Account email associated with the API token. Prompted for if omitted.
    /// Not required for --bearer.
    #[arg(long)]
    pub email: Option<String>,
    /// API token to store securely (falls back to ATLASSIAN_API_TOKEN env or interactive prompt).
    // `hide_env_values` keeps `--help` from printing the token itself; clap
    // shows the variable's value by default.
    #[arg(long, env = "ATLASSIAN_API_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
    /// Mark this profile as the default one.
    #[arg(long)]
    pub default: bool,
    /// Login for Bitbucket (uses api.bitbucket.org, no base_url required).
    #[arg(long)]
    pub bitbucket: bool,
    /// Use Bearer auth (for repository/workspace/project access tokens). Requires --bitbucket.
    #[arg(long, requires = "bitbucket")]
    pub bearer: bool,
    /// Bitbucket workspace slug (optional, for --bitbucket mode).
    #[arg(long, requires = "bitbucket")]
    pub workspace: Option<String>,
}

impl LoginArgs {
    /// Profile name, resolved by `login` before anything else runs.
    fn profile_name(&self) -> &str {
        self.profile.as_deref().unwrap_or_default()
    }
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
        AuthCommand::Status(args) => auth_status(args, config, renderer).await,
        AuthCommand::Whoami(args) => whoami(args, config).await,
        AuthCommand::Test(args) => test_auth(args, config).await,
    }
}

/// Take a value from a flag, or ask for it interactively.
///
/// `auth login` used to reject the bare command, even though the CLI's own
/// errors and the published guides both tell users to run exactly that. Only a
/// terminal gets a prompt: without one this stays a hard error, so scripts and
/// CI keep failing loudly rather than hanging on stdin.
fn resolve_value(existing: Option<String>, label: &str, flag: &str) -> Result<String> {
    if let Some(value) = existing {
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
    }

    if !std::io::stdin().is_terminal() {
        return Err(anyhow!(
            "Missing --{flag}. Pass it on the command line (no terminal available to prompt for {label})."
        ));
    }

    // Prompt on stderr so stdout stays clean for redirection.
    eprint!("{label}: ");
    std::io::stderr()
        .flush()
        .context("Failed to write prompt")?;
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .with_context(|| format!("Failed to read {label}"))?;

    let value = line.trim().to_string();
    if value.is_empty() {
        return Err(anyhow!("{label} cannot be empty"));
    }
    Ok(value)
}

fn login(mut args: LoginArgs, config: &mut Config, config_path: Option<&Path>) -> Result<()> {
    // Migrate any existing plaintext credentials to encrypted storage
    if let Ok(count) = atlassian_cli_auth::migrate_plaintext_to_encrypted() {
        if count > 0 {
            tracing::info!(
                credentials_migrated = count,
                "Migrated plaintext credentials to encrypted storage"
            );
        }
    }

    // The CLI's own errors say "Run `atlassian-cli auth login`", and the
    // published guides say the same, so the bare command has to work. Missing
    // values are prompted for on a terminal and remain hard errors without one,
    // so scripted use keeps failing loudly.
    args.profile = Some(resolve_value(
        args.profile.clone(),
        "Profile name",
        "profile",
    )?);
    if args.profile_name().trim().is_empty() {
        return Err(anyhow!("Profile name cannot be empty"));
    }

    if !args.bitbucket {
        args.base_url = Some(resolve_value(
            args.base_url.clone(),
            "Atlassian site base URL (e.g. https://example.atlassian.net)",
            "base-url",
        )?);
    }
    if !args.bearer {
        args.email = Some(resolve_value(args.email.clone(), "Account email", "email")?);
    }

    let token = match &args.token {
        Some(token) if !token.trim().is_empty() => token.trim().to_owned(),
        _ => read_token_from_stdin(&args).context("Failed to read token from prompt")?,
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

    let base_url = atlassian_cli_api::normalize_base_url(base_url);

    let email = args
        .email
        .as_ref()
        .ok_or_else(|| anyhow!("--email is required for Jira/Confluence login"))?;

    let profile_entry = config
        .profiles
        .entry(args.profile_name().to_string())
        .or_default();
    profile_entry.base_url = Some(base_url.to_string());
    profile_entry.email = Some(email.clone());
    profile_entry.api_token = None;

    if args.default || config.default_profile.is_none() {
        config.default_profile = Some(args.profile_name().to_string());
    }

    let secret_key = token_key(args.profile_name());
    atlassian_cli_auth::set_secret_encrypted(&secret_key, token)
        .context("Failed to store token in encrypted credentials file")?;

    config
        .save(config_path)
        .context("Unable to persist configuration file")?;

    tracing::info!(
        profile = %args.profile_name(),
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
    let profile_entry = config
        .profiles
        .entry(args.profile_name().to_string())
        .or_default();

    // Update workspace if provided
    if args.workspace.is_some() {
        profile_entry.workspace = args.workspace.clone();
    }

    // Set email if provided (not required for bearer tokens)
    if let Some(email) = &args.email {
        profile_entry.email = Some(email.clone());
    }

    // Store token type for bearer auth
    if args.bearer {
        profile_entry.bitbucket_token_type = Some("bearer".to_string());
        tracing::debug!("Storing Bitbucket token with Bearer auth type");
    } else {
        // Ensure email is set for basic auth
        if args.email.is_none() {
            return Err(anyhow!(
                "--email is required for Bitbucket Basic auth. \
                Use --bearer for repository/workspace access tokens (no email needed)."
            ));
        }
        profile_entry.bitbucket_token_type = None; // basic is default
    }

    if args.default || config.default_profile.is_none() {
        config.default_profile = Some(args.profile_name().to_string());
    }

    // Store Bitbucket token with _bitbucket suffix
    let secret_key = bitbucket_token_key(args.profile_name());
    atlassian_cli_auth::set_secret_encrypted(&secret_key, token)
        .context("Failed to store Bitbucket token in encrypted credentials file")?;

    config
        .save(config_path)
        .context("Unable to persist configuration file")?;

    let auth_type = if args.bearer { "Bearer" } else { "Basic" };
    tracing::info!(
        profile = %args.profile_name(),
        workspace = ?args.workspace,
        auth_type = auth_type,
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
            config.default_profile = None; // Force explicit re-selection
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
        bitbucket_auth: &'a str,
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

        let bitbucket_auth = if !has_bitbucket_token {
            ""
        } else if is_bitbucket_bearer(config, name) {
            "bearer"
        } else {
            "basic"
        };

        let row = Row {
            name,
            base_url: profile.base_url.as_deref().unwrap_or(""),
            email: profile.email.as_deref().unwrap_or(""),
            has_jira_token,
            has_bitbucket_token,
            bitbucket_auth,
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

fn read_token_from_stdin(args: &LoginArgs) -> Result<String> {
    use std::io::{self, Write};

    if args.bitbucket && args.bearer {
        println!("Enter a repository, workspace, or project access token.");
        println!("Create at: Repository/Workspace settings -> Access tokens");
    } else if args.bitbucket {
        println!("App passwords are deprecated. Use Bitbucket API tokens instead.");
        println!("Create at: https://id.atlassian.com/manage-profile/security/api-tokens");
        println!("  -> Click 'Create API token' -> Select 'Bitbucket' as the app -> Assign scopes");
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

    if args.bitbucket {
        let is_bearer = is_bitbucket_bearer(config, profile_name);
        let email = profile.email.as_deref().unwrap_or("");
        test_bitbucket_auth(profile_name, email, is_bearer).await
    } else {
        let email = profile.email.as_deref().context("Profile missing email")?;
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

async fn test_bitbucket_auth(profile_name: &str, email: &str, is_bearer: bool) -> Result<()> {
    let token = get_bitbucket_token(profile_name).ok_or_else(|| {
        anyhow!(
            "No Bitbucket token found for profile '{profile_name}'. \
            Set BITBUCKET_TOKEN or ATLASSIAN_CLI_BITBUCKET_TOKEN_{} env var, \
            or run `atlassian-cli auth login --bitbucket`\n\n\
            Hint: App passwords are deprecated. Use Bitbucket API tokens instead.\n\
            Create at: https://id.atlassian.com/manage-profile/security/api-tokens\n\
              -> Select 'Bitbucket' as the app when creating the token.",
            profile_name.to_uppercase()
        )
    })?;

    let auth_type = if is_bearer { "Bearer" } else { "Basic" };
    println!(
        "Testing Bitbucket authentication for profile '{}' ({} auth)...",
        profile_name, auth_type
    );

    let client = if is_bearer {
        atlassian_cli_api::ApiClient::new(BITBUCKET_API_URL)?.with_bearer_token(&token)
    } else {
        atlassian_cli_api::ApiClient::new(BITBUCKET_API_URL)?.with_basic_auth(email, &token)
    };

    // Bearer tokens (access tokens) can't use /2.0/user — use /2.0/workspaces instead
    if is_bearer {
        let result: Result<serde_json::Value> = client
            .get("/2.0/workspaces")
            .await
            .context("Bitbucket Bearer authentication test failed");

        match result {
            Ok(data) => {
                println!("Bitbucket authentication successful! (Bearer)");
                println!("   Profile: {}", profile_name);
                if let Some(workspaces) = data["values"].as_array() {
                    for ws in workspaces.iter().take(3) {
                        if let Some(slug) = ws["slug"].as_str() {
                            println!("   Workspace: {}", slug);
                        }
                    }
                }
                Ok(())
            }
            Err(e) => {
                println!("Bitbucket Bearer authentication failed: {}", e);
                Err(e)
            }
        }
    } else {
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
                println!();
                println!("Hint: App passwords are deprecated (disabled Jun 2026).");
                println!("Use Bitbucket API tokens: https://id.atlassian.com/manage-profile/security/api-tokens");
                println!("  -> Select 'Bitbucket' as the app when creating the token.");
                println!();
                println!("For repository/workspace access tokens, use --bearer flag:");
                println!("  atlassian-cli auth login --bitbucket --bearer --token <TOKEN>");
                Err(e)
            }
        }
    }
}

async fn auth_status(args: StatusArgs, config: &Config, renderer: &OutputRenderer) -> Result<()> {
    let (profile_name, profile) = config
        .resolve_profile(args.profile.as_deref())
        .context("No profile found. Use `atlassian-cli auth login` to create one.")?;

    let email = profile.email.as_deref().unwrap_or("");

    #[derive(Serialize)]
    struct ServiceStatus {
        service: String,
        status: String,
        details: String,
    }

    let mut statuses = Vec::new();

    // Check Jira/Confluence
    let jira_status = if let Some(base_url) = profile.base_url.as_deref() {
        if let Some(token) = get_token(profile_name) {
            let client = atlassian_cli_api::ApiClient::new(base_url)
                .ok()
                .map(|c| c.with_basic_auth(email, &token));

            if let Some(client) = client {
                match client.get::<serde_json::Value>("/rest/api/3/myself").await {
                    Ok(_) => ServiceStatus {
                        service: "Jira/Confluence".to_string(),
                        status: "OK".to_string(),
                        details: base_url.to_string(),
                    },
                    Err(e) => ServiceStatus {
                        service: "Jira/Confluence".to_string(),
                        status: "FAILED".to_string(),
                        details: format!("{}", e),
                    },
                }
            } else {
                ServiceStatus {
                    service: "Jira/Confluence".to_string(),
                    status: "FAILED".to_string(),
                    details: "Invalid base URL".to_string(),
                }
            }
        } else {
            ServiceStatus {
                service: "Jira/Confluence".to_string(),
                status: "N/A".to_string(),
                details: "No token configured".to_string(),
            }
        }
    } else {
        ServiceStatus {
            service: "Jira/Confluence".to_string(),
            status: "N/A".to_string(),
            details: "No base_url configured".to_string(),
        }
    };
    if !args.configured_only || jira_status.status != "N/A" {
        statuses.push(jira_status);
    }

    // Check Bitbucket
    let bb_is_bearer = is_bitbucket_bearer(config, profile_name);
    let bb_status = if let Some(token) = get_bitbucket_token(profile_name) {
        let client = if bb_is_bearer {
            atlassian_cli_api::ApiClient::new(BITBUCKET_API_URL)
                .ok()
                .map(|c| c.with_bearer_token(&token))
        } else {
            atlassian_cli_api::ApiClient::new(BITBUCKET_API_URL)
                .ok()
                .map(|c| c.with_basic_auth(email, &token))
        };

        if let Some(client) = client {
            // Bearer tokens can't use /2.0/user, use /2.0/workspaces instead
            let endpoint = if bb_is_bearer {
                "/2.0/workspaces"
            } else {
                "/2.0/user"
            };
            match client.get::<serde_json::Value>(endpoint).await {
                Ok(data) => {
                    let details = if bb_is_bearer {
                        let ws_count = data["values"].as_array().map(|v| v.len()).unwrap_or(0);
                        format!("bearer, {} workspace(s) accessible", ws_count)
                    } else {
                        let username = data["username"].as_str().unwrap_or("unknown");
                        format!("user: {}", username)
                    };
                    ServiceStatus {
                        service: "Bitbucket".to_string(),
                        status: "OK".to_string(),
                        details,
                    }
                }
                Err(e) => ServiceStatus {
                    service: "Bitbucket".to_string(),
                    status: "FAILED".to_string(),
                    details: format!("{}", e),
                },
            }
        } else {
            ServiceStatus {
                service: "Bitbucket".to_string(),
                status: "FAILED".to_string(),
                details: "Client init failed".to_string(),
            }
        }
    } else {
        ServiceStatus {
            service: "Bitbucket".to_string(),
            status: "N/A".to_string(),
            details: "Not configured".to_string(),
        }
    };
    if !args.configured_only || bb_status.status != "N/A" {
        statuses.push(bb_status);
    }

    // Check OpsGenie (from profile config or env var)
    let og_status =
        if profile.opsgenie_api_key.is_some() || std::env::var("OPSGENIE_API_KEY").is_ok() {
            ServiceStatus {
                service: "OpsGenie".to_string(),
                status: "CONFIGURED".to_string(),
                details: "API key present (use `opsgenie alert list` to test)".to_string(),
            }
        } else {
            ServiceStatus {
                service: "OpsGenie".to_string(),
                status: "N/A".to_string(),
                details: "Not configured".to_string(),
            }
        };
    if !args.configured_only || og_status.status != "N/A" {
        statuses.push(og_status);
    }

    // Check Bamboo
    let bamboo_status = if profile.bamboo_base_url.is_some() || profile.base_url.is_some() {
        if get_token(profile_name).is_some() {
            let base_url = profile
                .bamboo_base_url
                .as_deref()
                .or(profile.base_url.as_deref())
                .unwrap_or("");
            ServiceStatus {
                service: "Bamboo".to_string(),
                status: "CONFIGURED".to_string(),
                details: format!("{} (use `bamboo plan list` to test)", base_url),
            }
        } else {
            ServiceStatus {
                service: "Bamboo".to_string(),
                status: "N/A".to_string(),
                details: "No token configured".to_string(),
            }
        }
    } else {
        ServiceStatus {
            service: "Bamboo".to_string(),
            status: "N/A".to_string(),
            details: "Not configured".to_string(),
        }
    };
    if !args.configured_only || bamboo_status.status != "N/A" {
        statuses.push(bamboo_status);
    }

    println!("Profile: {}", profile_name);

    renderer.render_list_or_empty(&statuses, "No services configured.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlassian_cli_config::Profile;

    #[test]
    fn test_is_bitbucket_bearer_default_is_false() {
        let mut config = Config::default();
        config
            .profiles
            .insert("test".to_string(), Profile::default());
        assert!(!is_bitbucket_bearer(&config, "test"));
    }

    #[test]
    fn test_is_bitbucket_bearer_when_set() {
        let mut config = Config::default();
        config.profiles.insert(
            "ci".to_string(),
            Profile {
                bitbucket_token_type: Some("bearer".to_string()),
                ..Default::default()
            },
        );
        assert!(is_bitbucket_bearer(&config, "ci"));
    }

    #[test]
    fn test_is_bitbucket_bearer_basic_is_false() {
        let mut config = Config::default();
        config.profiles.insert(
            "work".to_string(),
            Profile {
                bitbucket_token_type: Some("basic".to_string()),
                ..Default::default()
            },
        );
        assert!(!is_bitbucket_bearer(&config, "work"));
    }

    #[test]
    fn test_is_bitbucket_bearer_nonexistent_profile() {
        let config = Config::default();
        assert!(!is_bitbucket_bearer(&config, "nonexistent"));
    }
}
