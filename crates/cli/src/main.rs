mod commands;
mod query;

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use atlassian_cli_api::ApiClient;
use atlassian_cli_auth::{token_key, CredentialStore};
use atlassian_cli_config::{migrate_config_if_needed, Config, MigrationResult};
use atlassian_cli_output::{OutputFormat, OutputRenderer};
use clap::{Parser, Subcommand};
use commands::auth::{self, AuthCommand};
use commands::bitbucket::utils::extract_workspace_from_url;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "atlassian-cli", version, about = "Unified Atlassian Cloud CLI", long_about = None)]
struct Cli {
    /// Profile to use from config file
    #[arg(short, long)]
    profile: Option<String>,

    /// Path to config file (defaults to ~/.atlassian-cli/config.yaml)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Output format for command results
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    output: OutputFormat,

    /// Enable verbose logging
    #[arg(long)]
    debug: bool,

    #[command(subcommand)]
    command: AtlassianCommand,
}

#[derive(Subcommand, Debug, Clone)]
enum AtlassianCommand {
    /// Jira commands
    Jira(commands::jira::JiraArgs),
    /// Confluence commands
    Confluence(commands::confluence::ConfluenceArgs),
    /// Bitbucket commands
    Bitbucket(commands::bitbucket::BitbucketArgs),
    /// Jira Service Management commands
    Jsm(commands::jsm::JsmArgs),
    /// Opsgenie commands
    Opsgenie(commands::opsgenie::OpsgenieArgs),
    /// Bamboo commands
    Bamboo(commands::bamboo::BambooArgs),
    /// Authentication commands
    #[command(subcommand)]
    Auth(AuthCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.debug)?;

    // Perform config directory migration if needed (only when no custom path specified)
    if cli.config.is_none() {
        handle_migration();
    }

    let config_path = cli.config.clone();
    let mut config = Config::load(config_path.as_ref())?;
    let renderer = OutputRenderer::new(cli.output);
    let credential_store = CredentialStore::new("atlassian-cli");

    let profile_ctx = if matches!(cli.command, AtlassianCommand::Auth(_)) {
        None
    } else {
        Some(resolve_active_profile(
            &config,
            cli.profile.as_deref(),
            &credential_store,
        )?)
    };

    match cli.command {
        AtlassianCommand::Jira(args) => {
            let profile = profile_ctx
                .as_ref()
                .expect("profile context is available for product commands");
            let client = build_product_client(profile)?;
            commands::jira::execute(args, client, &renderer).await?
        }
        AtlassianCommand::Confluence(args) => {
            let profile = profile_ctx
                .as_ref()
                .expect("profile context is available for product commands");
            let client = build_product_client(profile)?;
            commands::confluence::execute(args, client, &renderer).await?
        }
        AtlassianCommand::Bitbucket(args) => {
            let profile = profile_ctx
                .as_ref()
                .expect("profile context is available for product commands");
            let client = build_bitbucket_client(profile)?;
            commands::bitbucket::execute(args, client, &renderer, profile.workspace.as_deref())
                .await?
        }
        AtlassianCommand::Jsm(args) => {
            let profile = profile_ctx
                .as_ref()
                .expect("profile context is available for product commands");
            let client = build_product_client(profile)?;
            commands::jsm::execute(
                args,
                commands::jsm::JsmContext {
                    client,
                    renderer: &renderer,
                },
            )
            .await?
        }
        AtlassianCommand::Opsgenie(args) => commands::opsgenie::execute(args).await?,
        AtlassianCommand::Bamboo(args) => commands::bamboo::execute(args).await?,
        AtlassianCommand::Auth(command) => auth::handle(
            command,
            &mut config,
            config_path.as_deref(),
            &credential_store,
            &renderer,
        )?,
    }

    Ok(())
}

fn init_tracing(debug: bool) -> Result<()> {
    let default = if debug {
        "info,atlassian-cli=debug"
    } else {
        "info"
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init()
        .map_err(|err| anyhow!("failed to initialize logger: {err}"))
}

struct ActiveProfile {
    base_url: String,
    email: String,
    token: String,
    bitbucket_token: Option<String>,
    workspace: Option<String>,
}

fn handle_migration() {
    match migrate_config_if_needed() {
        MigrationResult::Migrated { from, to } => {
            eprintln!(
                "Config migrated from {} to {}\nThe old directory can be safely removed.",
                from.display(),
                to.display()
            );
        }
        MigrationResult::Failed(e) => {
            eprintln!("Warning: Config migration failed: {}", e);
        }
        MigrationResult::NotNeeded => {}
    }
}

fn resolve_active_profile(
    config: &Config,
    requested: Option<&str>,
    store: &CredentialStore,
) -> Result<ActiveProfile> {
    let (name, profile) = config
        .resolve_profile(requested)
        .ok_or_else(|| anyhow!("No profile configured. Run `atlassian-cli auth login` first."))?;

    let base_url = profile
        .base_url
        .clone()
        .ok_or_else(|| anyhow!("Profile '{name}' is missing a base_url."))?;
    let email = profile
        .email
        .clone()
        .ok_or_else(|| anyhow!("Profile '{name}' is missing an email."))?;

    // Multi-tier token lookup: profile-specific env var → generic env var → keyring
    let token = {
        // 1. Check profile-specific env var: ATLASSIAN_CLI_TOKEN_{PROFILE}
        let profile_env_var = format!("ATLASSIAN_CLI_TOKEN_{}", name.to_uppercase());
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
                // 3. Try keyring as fallback
                let secret_key = token_key(name);
                store.get_secret(&secret_key).ok().flatten()
            })
            .ok_or_else(|| {
                anyhow!(
                    "No token found for profile '{name}'. Set ATLASSIAN_CLI_TOKEN_{} env var or run `atlassian-cli auth login --profile {name}`",
                    name.to_uppercase()
                )
            })?
    };

    // Bitbucket-specific token lookup (in priority order):
    // 1. ATLASSIAN_CLI_BITBUCKET_TOKEN_{PROFILE}
    // 2. ATLASSIAN_BITBUCKET_TOKEN
    // 3. BITBUCKET_TOKEN
    let bitbucket_token = {
        let profile_env_var = format!("ATLASSIAN_CLI_BITBUCKET_TOKEN_{}", name.to_uppercase());
        std::env::var(&profile_env_var)
            .ok()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| {
                std::env::var("ATLASSIAN_BITBUCKET_TOKEN")
                    .ok()
                    .filter(|t| !t.trim().is_empty())
            })
            .or_else(|| {
                std::env::var("BITBUCKET_TOKEN")
                    .ok()
                    .filter(|t| !t.trim().is_empty())
            })
    };

    // Resolve workspace: explicit profile config, or infer from base_url
    let workspace = profile
        .workspace
        .clone()
        .or_else(|| extract_workspace_from_url(&base_url));

    Ok(ActiveProfile {
        base_url,
        email,
        token,
        bitbucket_token,
        workspace,
    })
}

fn build_product_client(profile: &ActiveProfile) -> Result<ApiClient> {
    Ok(ApiClient::new(&profile.base_url)?
        .with_basic_auth(profile.email.clone(), profile.token.clone()))
}

fn build_bitbucket_client(profile: &ActiveProfile) -> Result<ApiClient> {
    // Use Bitbucket-specific token if set, otherwise fall back to general token
    let token = profile.bitbucket_token.as_ref().unwrap_or(&profile.token);
    Ok(ApiClient::new("https://api.bitbucket.org")?
        .with_basic_auth(profile.email.clone(), token.clone()))
}
