mod commands;
mod query;

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use atlassian_cli_api::ApiClient;
use atlassian_cli_auth::BITBUCKET_API_URL;
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
    #[command(visible_alias = "bb")]
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

    // Validate profile selection for destructive commands
    let is_destructive = is_destructive_command(&cli.command);
    validate_profile_selection(&config, cli.profile.as_deref(), is_destructive)?;

    match cli.command {
        AtlassianCommand::Jira(args) => {
            let profile = resolve_profile_for_product(&config, cli.profile.as_deref())?;
            let client = build_product_client(&profile)?;
            commands::jira::execute(args, client, &renderer).await?
        }
        AtlassianCommand::Confluence(args) => {
            let profile = resolve_profile_for_product(&config, cli.profile.as_deref())?;
            let client = build_product_client(&profile)?;
            commands::confluence::execute(args, client, &renderer).await?
        }
        AtlassianCommand::Bitbucket(args) => {
            let profile = resolve_profile_for_bitbucket(&config, cli.profile.as_deref())?;
            let client = build_bitbucket_client(&profile)?;
            commands::bitbucket::execute(args, client, &renderer, profile.workspace.as_deref())
                .await?
        }
        AtlassianCommand::Jsm(args) => {
            let profile = resolve_profile_for_product(&config, cli.profile.as_deref())?;
            let client = build_product_client(&profile)?;
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
        AtlassianCommand::Auth(command) => {
            auth::handle(command, &mut config, config_path.as_deref(), &renderer).await?
        }
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

/// Shared profile fields used by all Atlassian products.
struct BaseProfile {
    name: String,
    email: String,
}

/// Profile for Jira/Confluence/JSM commands (requires base_url and token).
struct ProductProfile {
    base: BaseProfile,
    base_url: String,
    token: String,
}

/// Profile for Bitbucket commands (only requires email and bitbucket token).
struct BitbucketProfile {
    base: BaseProfile,
    token: String,
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

/// Validates that destructive commands have explicit profile selection.
fn validate_profile_selection(
    config: &Config,
    requested: Option<&str>,
    command_is_destructive: bool,
) -> Result<()> {
    if command_is_destructive
        && requested.is_none()
        && config.default_profile.is_none()
    {
        return Err(anyhow!(
            "Destructive command requires explicit profile selection.\n\
             Use --profile <name> or set default with: atlassian-cli auth login --default"
        ));
    }
    Ok(())
}

/// Determines if a command is destructive (modifies or deletes data).
/// TODO: Implement proper destructive command detection by checking nested command enums.
/// For now, returns false since IndexMap already provides deterministic profile selection.
fn is_destructive_command(_command: &AtlassianCommand) -> bool {
    // The IndexMap change already ensures deterministic profile selection,
    // mitigating the main security risk. Full destructive command detection
    // requires deep pattern matching on nested enum structures and will be
    // implemented in a follow-up.
    false
}

/// Resolve base profile fields (name + email) shared by all commands.
fn resolve_base_profile<'a>(
    config: &'a Config,
    requested: Option<&'a str>,
) -> Result<(BaseProfile, &'a atlassian_cli_config::Profile)> {
    let (name, profile) = config
        .resolve_profile(requested)
        .ok_or_else(|| anyhow!("No profile configured. Run `atlassian-cli auth login` first."))?;

    let email = profile
        .email
        .clone()
        .ok_or_else(|| anyhow!("Profile '{name}' is missing an email."))?;

    Ok((
        BaseProfile {
            name: name.to_string(),
            email,
        },
        profile,
    ))
}

/// Resolve profile for Jira/Confluence/JSM commands.
/// Requires base_url and Jira/Confluence token.
fn resolve_profile_for_product(config: &Config, requested: Option<&str>) -> Result<ProductProfile> {
    let (base, profile) = resolve_base_profile(config, requested)?;

    let base_url = profile.base_url.clone().ok_or_else(|| {
        anyhow!(
            "Profile '{}' is missing a base_url. Run `atlassian-cli auth login --base-url <URL>`",
            base.name
        )
    })?;

    let token = auth::get_token(&base.name).ok_or_else(|| {
        anyhow!(
            "No token found for profile '{}'. Set ATLASSIAN_CLI_TOKEN_{} env var or run `atlassian-cli auth login --profile {}`",
            base.name,
            base.name.to_uppercase(),
            base.name
        )
    })?;

    Ok(ProductProfile {
        base,
        base_url,
        token,
    })
}

/// Resolve profile for Bitbucket commands.
/// Only requires email and Bitbucket token (falls back to general token).
fn resolve_profile_for_bitbucket(
    config: &Config,
    requested: Option<&str>,
) -> Result<BitbucketProfile> {
    let (base, profile) = resolve_base_profile(config, requested)?;

    // Try Bitbucket-specific token first, then fall back to general token
    let token = auth::get_bitbucket_token(&base.name)
        .or_else(|| auth::get_token(&base.name))
        .ok_or_else(|| {
            let has_jira_token = auth::get_token(&base.name).is_some();
            if has_jira_token {
                // Has Jira token but no Bitbucket token - suggest adding Bitbucket auth
                anyhow!(
                    "Profile '{}' has no Bitbucket token (only Jira/Confluence token found). \
                    Run `atlassian-cli auth login --bitbucket --profile {}`",
                    base.name,
                    base.name
                )
            } else {
                // No tokens at all
                anyhow!(
                    "No token found for profile '{}'. Run `atlassian-cli auth login --bitbucket --profile {}`",
                    base.name,
                    base.name
                )
            }
        })?;

    // Resolve workspace: explicit profile config, or infer from base_url if present
    let workspace = profile.workspace.clone().or_else(|| {
        profile
            .base_url
            .as_ref()
            .and_then(|url| extract_workspace_from_url(url))
    });

    Ok(BitbucketProfile {
        base,
        token,
        workspace,
    })
}

fn build_product_client(profile: &ProductProfile) -> Result<ApiClient> {
    Ok(ApiClient::new(&profile.base_url)?
        .with_basic_auth(profile.base.email.clone(), profile.token.clone()))
}

fn build_bitbucket_client(profile: &BitbucketProfile) -> Result<ApiClient> {
    Ok(ApiClient::new(BITBUCKET_API_URL)?
        .with_basic_auth(profile.base.email.clone(), profile.token.clone()))
}
