use std::path::PathBuf;

use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};

mod agents;
mod artifacts;
mod branches;
mod builds;
mod deployments;
mod plans;
mod projects;
pub mod utils;

pub use utils::BambooContext;

#[derive(Args, Debug, Clone)]
pub struct BambooArgs {
    #[command(subcommand)]
    command: BambooCommands,
}

#[derive(Subcommand, Debug, Clone)]
enum BambooCommands {
    /// Project operations.
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Plan operations.
    #[command(subcommand)]
    Plan(PlanCommands),

    /// Branch operations.
    #[command(subcommand)]
    Branch(BranchCommands),

    /// Build operations.
    #[command(subcommand)]
    Build(BuildCommands),

    /// Deployment operations.
    #[command(subcommand)]
    Deploy(DeployCommands),

    /// Agent operations.
    #[command(subcommand)]
    Agent(AgentCommands),

    /// Artifact operations.
    #[command(subcommand)]
    Artifact(ArtifactCommands),

    /// Server information.
    Info,

    /// Build queue operations.
    Queue,
}

#[derive(Subcommand, Debug, Clone)]
enum ProjectCommands {
    /// List projects.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get project details.
    Get {
        #[arg(value_name = "KEY")]
        key: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum PlanCommands {
    /// List plans.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get plan details.
    Get {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Enable a plan.
    Enable {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Disable a plan.
    Disable {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Favorite a plan.
    Favorite {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Unfavorite a plan.
    Unfavorite {
        #[arg(value_name = "KEY")]
        key: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum BranchCommands {
    /// List branches for a plan.
    List {
        #[arg(value_name = "PLAN_KEY")]
        plan_key: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get branch details.
    Get {
        #[arg(value_name = "BRANCH_KEY")]
        branch_key: String,
    },
    /// Create a branch plan.
    Create {
        #[arg(value_name = "PLAN_KEY")]
        plan_key: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        vcs_branch: Option<String>,
    },
    /// Delete a branch plan.
    Delete {
        #[arg(value_name = "BRANCH_KEY")]
        branch_key: String,
    },
    /// Enable a branch plan.
    Enable {
        #[arg(value_name = "BRANCH_KEY")]
        branch_key: String,
    },
    /// Disable a branch plan.
    Disable {
        #[arg(value_name = "BRANCH_KEY")]
        branch_key: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum BuildCommands {
    /// List builds for a plan.
    List {
        #[arg(value_name = "PLAN_KEY")]
        plan_key: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get build details.
    Get {
        #[arg(value_name = "BUILD_KEY")]
        build_key: String,
    },
    /// Get latest build for a plan.
    Latest {
        #[arg(value_name = "PLAN_KEY")]
        plan_key: String,
    },
    /// Run a build.
    Run {
        #[arg(value_name = "PLAN_KEY")]
        plan_key: String,
        #[arg(long)]
        stage: Option<String>,
        #[arg(long)]
        revision: Option<String>,
    },
    /// Stop a running build.
    Stop {
        #[arg(value_name = "BUILD_KEY")]
        build_key: String,
    },
    /// Get build logs.
    Logs {
        #[arg(value_name = "BUILD_KEY")]
        build_key: String,
        #[arg(long)]
        job: Option<String>,
    },
    /// Add comment to a build.
    Comment {
        #[arg(value_name = "BUILD_KEY")]
        build_key: String,
        #[arg(long)]
        message: String,
    },
    /// Add label to a build.
    AddLabel {
        #[arg(value_name = "BUILD_KEY")]
        build_key: String,
        #[arg(long)]
        label: String,
    },
    /// Remove label from a build.
    RemoveLabel {
        #[arg(value_name = "BUILD_KEY")]
        build_key: String,
        #[arg(long)]
        label: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum DeployCommands {
    /// List deployment projects.
    Projects {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get deployment project details.
    Project {
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// List environments for a deployment project.
    Environments {
        #[arg(value_name = "PROJECT_ID")]
        project_id: i64,
    },
    /// Get environment details.
    Environment {
        #[arg(value_name = "ENV_ID")]
        env_id: i64,
    },
    /// List deployment results for an environment.
    Results {
        #[arg(value_name = "ENV_ID")]
        env_id: i64,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Trigger a deployment.
    Trigger {
        #[arg(value_name = "ENV_ID")]
        env_id: i64,
        #[arg(long)]
        version_id: i64,
    },
    /// List versions for a deployment project.
    Versions {
        #[arg(value_name = "PROJECT_ID")]
        project_id: i64,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum AgentCommands {
    /// List agents.
    List,
    /// Get agent details.
    Get {
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// Enable an agent.
    Enable {
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// Disable an agent.
    Disable {
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// List agent capabilities.
    Capabilities {
        #[arg(value_name = "ID")]
        id: i64,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum ArtifactCommands {
    /// List artifacts for a build.
    List {
        #[arg(value_name = "BUILD_KEY")]
        build_key: String,
    },
    /// Download an artifact.
    Download {
        #[arg(value_name = "BUILD_KEY")]
        build_key: String,
        /// Artifact name to download.
        #[arg(long)]
        name: String,
        /// Output file path.
        #[arg(long, short)]
        output: PathBuf,
    },
}

/// Execute Bamboo command.
pub async fn execute(args: BambooArgs, client: ApiClient, renderer: &OutputRenderer) -> Result<()> {
    let ctx = BambooContext { client, renderer };

    match args.command {
        BambooCommands::Project(cmd) => match cmd {
            ProjectCommands::List { limit } => projects::list_projects(&ctx, limit).await,
            ProjectCommands::Get { key } => projects::get_project(&ctx, &key).await,
        },
        BambooCommands::Plan(cmd) => match cmd {
            PlanCommands::List { limit } => plans::list_plans(&ctx, limit).await,
            PlanCommands::Get { key } => plans::get_plan(&ctx, &key).await,
            PlanCommands::Enable { key } => plans::enable_plan(&ctx, &key).await,
            PlanCommands::Disable { key } => plans::disable_plan(&ctx, &key).await,
            PlanCommands::Favorite { key } => plans::favorite_plan(&ctx, &key).await,
            PlanCommands::Unfavorite { key } => plans::unfavorite_plan(&ctx, &key).await,
        },
        BambooCommands::Branch(cmd) => match cmd {
            BranchCommands::List { plan_key, limit } => {
                branches::list_branches(&ctx, &plan_key, limit).await
            }
            BranchCommands::Get { branch_key } => branches::get_branch(&ctx, &branch_key).await,
            BranchCommands::Create {
                plan_key,
                name,
                vcs_branch,
            } => branches::create_branch(&ctx, &plan_key, &name, vcs_branch.as_deref()).await,
            BranchCommands::Delete { branch_key } => {
                branches::delete_branch(&ctx, &branch_key).await
            }
            BranchCommands::Enable { branch_key } => {
                branches::enable_branch(&ctx, &branch_key).await
            }
            BranchCommands::Disable { branch_key } => {
                branches::disable_branch(&ctx, &branch_key).await
            }
        },
        BambooCommands::Build(cmd) => match cmd {
            BuildCommands::List { plan_key, limit } => {
                builds::list_builds(&ctx, &plan_key, limit).await
            }
            BuildCommands::Get { build_key } => builds::get_build(&ctx, &build_key).await,
            BuildCommands::Latest { plan_key } => builds::get_latest(&ctx, &plan_key).await,
            BuildCommands::Run {
                plan_key,
                stage,
                revision,
            } => builds::run_build(&ctx, &plan_key, stage.as_deref(), revision.as_deref()).await,
            BuildCommands::Stop { build_key } => builds::stop_build(&ctx, &build_key).await,
            BuildCommands::Logs { build_key, job } => {
                builds::get_logs(&ctx, &build_key, job.as_deref()).await
            }
            BuildCommands::Comment { build_key, message } => {
                builds::add_comment(&ctx, &build_key, &message).await
            }
            BuildCommands::AddLabel { build_key, label } => {
                builds::add_label(&ctx, &build_key, &label).await
            }
            BuildCommands::RemoveLabel { build_key, label } => {
                builds::remove_label(&ctx, &build_key, &label).await
            }
        },
        BambooCommands::Deploy(cmd) => match cmd {
            DeployCommands::Projects { limit } => deployments::list_projects(&ctx, limit).await,
            DeployCommands::Project { id } => deployments::get_project(&ctx, id).await,
            DeployCommands::Environments { project_id } => {
                deployments::list_environments(&ctx, project_id).await
            }
            DeployCommands::Environment { env_id } => {
                deployments::get_environment(&ctx, env_id).await
            }
            DeployCommands::Results { env_id, limit } => {
                deployments::list_results(&ctx, env_id, limit).await
            }
            DeployCommands::Trigger { env_id, version_id } => {
                deployments::trigger_deployment(&ctx, env_id, version_id).await
            }
            DeployCommands::Versions { project_id, limit } => {
                deployments::list_versions(&ctx, project_id, limit).await
            }
        },
        BambooCommands::Agent(cmd) => match cmd {
            AgentCommands::List => agents::list_agents(&ctx).await,
            AgentCommands::Get { id } => agents::get_agent(&ctx, id).await,
            AgentCommands::Enable { id } => agents::enable_agent(&ctx, id).await,
            AgentCommands::Disable { id } => agents::disable_agent(&ctx, id).await,
            AgentCommands::Capabilities { id } => agents::list_capabilities(&ctx, id).await,
        },
        BambooCommands::Artifact(cmd) => match cmd {
            ArtifactCommands::List { build_key } => {
                artifacts::list_artifacts(&ctx, &build_key).await
            }
            ArtifactCommands::Download {
                build_key,
                name,
                output,
            } => artifacts::download_artifact(&ctx, &build_key, &name, &output).await,
        },
        BambooCommands::Info => agents::get_server_info(&ctx).await,
        BambooCommands::Queue => agents::list_queue(&ctx).await,
    }
}
