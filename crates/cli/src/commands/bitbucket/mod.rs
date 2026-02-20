use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};

// Submodules
mod branches;
mod bulk;
mod commits;
mod git;
mod permissions;
mod pipelines;
mod pullrequests;
mod repos;
mod time_parser;
pub mod utils;
mod variables;
mod webhooks;
mod workspaces;

use utils::BitbucketContext;

/// Helper to require a repo slug with a clear error message
fn require_repo(
    explicit: Option<&str>,
    git_detected: Option<&str>,
    cmd: &str,
) -> anyhow::Result<String> {
    explicit.or(git_detected).map(String::from).ok_or_else(|| {
        anyhow::anyhow!(
            "Repository required for '{cmd}'.\n\n\
             Not in a git directory with Bitbucket remote.\n\
             Use --repo <repo-slug> to specify the repository."
        )
    })
}

#[derive(Args, Debug, Clone)]
pub struct BitbucketArgs {
    /// Workspace slug (defaults to workspace configured in profile base URL host prefix).
    #[arg(long, global = true)]
    pub workspace: Option<String>,

    /// Repository slug (auto-detected from git remote if not specified).
    #[arg(long, global = true)]
    pub repo: Option<String>,

    #[command(subcommand)]
    command: BitbucketCommands,
}

#[derive(Subcommand, Debug, Clone)]
enum BitbucketCommands {
    /// Repository operations.
    #[command(subcommand)]
    Repo(RepoCommands),

    /// Branch operations.
    #[command(subcommand)]
    Branch(BranchCommands),

    /// Pull request operations.
    #[command(subcommand)]
    Pr(PrCommands),

    /// Workspace operations.
    #[command(subcommand)]
    Workspace(WorkspaceCommands),

    /// Project operations.
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Pipeline operations.
    #[command(subcommand)]
    Pipeline(PipelineCommands),

    /// Webhook operations.
    #[command(subcommand)]
    Webhook(WebhookCommands),

    /// SSH deploy key operations.
    #[command(subcommand)]
    SshKey(SshKeyCommands),

    /// Repository permission operations.
    #[command(subcommand)]
    Permission(PermissionCommands),

    /// Commit operations.
    #[command(subcommand)]
    Commit(CommitCommands),

    /// Bulk operations.
    #[command(subcommand)]
    Bulk(BulkCommands),

    /// Show current authenticated Bitbucket user.
    Whoami,
}

#[derive(Subcommand, Debug, Clone)]
enum RepoCommands {
    /// List repositories inside a workspace.
    #[command(
        long_about = "List repositories inside a workspace.\n\nExamples:\n  bb repo list\n  bb repo list --limit 50\n  bb repo list --workspace my-team"
    )]
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Show repository metadata.
    #[command(long_about = "Show repository metadata.\n\nExamples:\n  bb repo get my-repo")]
    Get {
        /// Repository slug (e.g., my-repo)
        slug: String,
    },
    /// Create a new repository.
    #[command(
        long_about = "Create a new repository.\n\nExamples:\n  bb repo create my-new-repo\n  bb repo create my-repo --name \"My Repository\" --description \"Project repo\" --private"
    )]
    Create {
        /// Repository slug (URL-friendly name, e.g., my-repo)
        slug: String,
        /// Repository display name
        #[arg(long)]
        name: Option<String>,
        /// Repository description
        #[arg(long)]
        description: Option<String>,
        /// Make repository private
        #[arg(long)]
        private: bool,
        /// Project key to associate with.
        #[arg(long)]
        project: Option<String>,
    },
    /// Update repository metadata.
    Update {
        /// Repository slug.
        slug: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// Programming language.
        #[arg(long)]
        language: Option<String>,
    },
    /// Delete a repository.
    Delete {
        /// Repository slug.
        slug: String,
        /// Skip confirmation prompt.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum BranchCommands {
    /// List branches in a repository.
    List {
        /// Repository slug.
        repo: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get branch details.
    Get {
        /// Repository slug.
        repo: String,
        /// Branch name.
        branch: String,
    },
    /// Create a new branch.
    Create {
        /// Repository slug.
        repo: String,
        /// New branch name.
        branch: String,
        /// Source commit hash or branch name.
        #[arg(long)]
        from: String,
    },
    /// Delete a branch.
    Delete {
        /// Repository slug.
        repo: String,
        /// Branch name.
        branch: String,
        /// Skip confirmation prompt.
        #[arg(long)]
        force: bool,
    },
    /// Add branch protection (restriction).
    Protect {
        /// Repository slug.
        repo: String,
        /// Branch name pattern (e.g., "main", "release/*").
        #[arg(long)]
        pattern: String,
        /// Restriction kind (push, delete, force, restrict_merges).
        #[arg(long)]
        kind: String,
        /// Number of required approvals.
        #[arg(long)]
        approvals: Option<i32>,
    },
    /// Remove branch protection.
    Unprotect {
        /// Repository slug.
        repo: String,
        /// Branch restriction ID.
        restriction_id: i64,
    },
    /// List branch restrictions.
    Restrictions {
        /// Repository slug.
        repo: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum PrCommands {
    /// List pull requests for a repository.
    List {
        /// Repository slug.
        repo: String,
        #[arg(long, default_value = "OPEN")]
        state: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get pull request details.
    Get {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
    },
    /// Create a new pull request.
    Create {
        /// Repository slug.
        repo: String,
        /// PR title.
        #[arg(long)]
        title: String,
        /// Source branch.
        #[arg(long)]
        source: String,
        /// Destination branch.
        #[arg(long)]
        destination: String,
        /// PR description.
        #[arg(long)]
        description: Option<String>,
        /// Reviewer UUIDs (comma-separated).
        #[arg(long, value_delimiter = ',')]
        reviewers: Vec<String>,
    },
    /// Update pull request.
    Update {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Merge pull request.
    Merge {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
        /// Merge strategy: merge_commit, squash, or fast_forward.
        #[arg(long)]
        strategy: Option<String>,
        /// Merge commit message.
        #[arg(long)]
        message: Option<String>,
    },
    /// Decline/close pull request.
    Decline {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
    },
    /// Approve pull request.
    Approve {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
    },
    /// Remove approval from pull request.
    Unapprove {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
    },
    /// View pull request diff.
    Diff {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
    },
    /// List pull request comments.
    Comments {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
    },
    /// Add comment to pull request.
    Comment {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
        /// Comment text.
        #[arg(long)]
        text: String,
    },
    /// Add reviewers to pull request.
    Reviewers {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
        /// Reviewer UUIDs (comma-separated).
        #[arg(long, value_delimiter = ',')]
        add: Vec<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum WorkspaceCommands {
    /// List workspaces.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get workspace details.
    Get { slug: String },
}

#[derive(Subcommand, Debug, Clone)]
enum ProjectCommands {
    /// List projects in workspace.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get project details.
    Get { key: String },
    /// Create a new project.
    Create {
        /// Project key (uppercase).
        key: String,
        /// Project name.
        #[arg(long)]
        name: String,
        /// Project description.
        #[arg(long)]
        description: Option<String>,
        /// Make project private.
        #[arg(long)]
        private: bool,
    },
    /// Update project.
    Update {
        /// Project key.
        key: String,
        /// New name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete project.
    Delete {
        /// Project key.
        key: String,
        /// Skip confirmation.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum PipelineCommands {
    /// List pipelines.
    List {
        /// Maximum number of results.
        #[arg(long, default_value_t = 25)]
        limit: usize,
        /// Sort field (prefix with - for desc): created_on, -created_on.
        #[arg(long)]
        sort: Option<String>,
        /// Show N most recent pipelines (shorthand for --sort=-created_on --limit=N).
        #[arg(long)]
        recent: Option<usize>,
        /// Filter by branch name.
        #[arg(long)]
        branch: Option<String>,
        /// Filter pipelines created after this time (inclusive). Examples: 24h, 7d, 2024-01-01T00:00:00Z
        #[arg(long)]
        since: Option<String>,
        /// Filter pipelines created before this time (exclusive). Examples: 7d, 2024-12-01T00:00:00Z
        #[arg(long)]
        before: Option<String>,
        /// Filter by pull request number (uses PR's source branch).
        #[arg(long)]
        pr: Option<i64>,
        /// Fetch all pages (ignores --limit).
        #[arg(long)]
        all: bool,
        /// Show step summary for each pipeline (adds N API calls).
        #[arg(long)]
        steps: bool,
    },
    /// Get pipeline details.
    Get {
        /// Pipeline UUID or build number.
        pipeline_id: String,
        /// Show pipeline steps with status.
        #[arg(long)]
        steps: bool,
    },
    /// Get latest pipeline for current or specified branch.
    Latest {
        /// Branch name (auto-detected from current branch if not specified).
        #[arg(long)]
        branch: Option<String>,
        /// Show pipeline steps with status.
        #[arg(long)]
        steps: bool,
    },
    /// Trigger a new pipeline.
    Trigger {
        /// Branch or tag name.
        #[arg(long)]
        ref_name: String,
        /// Reference type (branch or tag).
        #[arg(long, default_value = "branch")]
        ref_type: String,
        /// Pipeline variables in KEY=VALUE format (can be repeated).
        #[arg(long = "var")]
        variables: Vec<String>,
        /// Mark all variables as secured.
        #[arg(long, default_value_t = false)]
        secured: bool,
    },
    /// Stop a running pipeline.
    Stop {
        /// Pipeline UUID or build number.
        pipeline_id: String,
    },
    /// Get pipeline logs.
    Logs {
        /// Pipeline UUID or build number.
        pipeline_id: String,
        /// Step UUID (optional - shows all steps if not specified).
        step_uuid: Option<String>,
        /// Filter by step name pattern.
        #[arg(long)]
        step: Option<String>,
        /// Grep pattern to filter log lines.
        #[arg(long)]
        grep: Option<String>,
        /// Case-insensitive grep.
        #[arg(long, short = 'i')]
        ignore_case: bool,
        /// Show only failed steps.
        #[arg(long)]
        failed_only: bool,
    },
    /// Watch a running pipeline until completion.
    Watch {
        /// Pipeline UUID or build number.
        pipeline_id: String,
        /// Poll interval in seconds.
        #[arg(long, default_value_t = 5)]
        interval: u64,
        /// Show pipeline steps with status.
        #[arg(long)]
        steps: bool,
    },
    /// List steps for a pipeline.
    Steps {
        /// Pipeline UUID or build number.
        pipeline_id: String,
    },
    /// Get latest pipeline status (JSON output, smart exit codes).
    Status {
        /// Show pipeline steps with status.
        #[arg(long)]
        steps: bool,
    },
    /// Re-run a pipeline with the same commit.
    Rerun {
        /// Pipeline UUID or build number (optional when using --pr).
        pipeline_id: Option<String>,
        /// Re-run from pull request (uses PR's source branch).
        #[arg(long)]
        pr: Option<i64>,
        /// Only re-run if there are failed steps (requires --pr).
        #[arg(long, requires = "pr")]
        failed_only: bool,
        /// Pipeline variables in KEY=VALUE format (can be repeated).
        #[arg(long = "var")]
        variables: Vec<String>,
        /// Mark all variables as secured.
        #[arg(long, default_value_t = false)]
        secured: bool,
    },
    /// Manage pipeline variables/secrets.
    #[command(subcommand)]
    Var(VarCommands),
    /// Manage deployment environments.
    #[command(subcommand)]
    Env(EnvCommands),
}

#[derive(Subcommand, Debug, Clone)]
enum VarCommands {
    /// List pipeline variables.
    #[command(
        long_about = "List pipeline variables.\n\nExamples:\n  bb pipeline var list\n  bb pipeline var list --workspace-level\n  bb pipeline var list --deployment staging"
    )]
    List {
        /// List workspace-level variables instead of repository variables.
        #[arg(long, conflicts_with = "deployment")]
        workspace_level: bool,
        /// List variables for a deployment environment (name or UUID).
        #[arg(long, conflicts_with = "workspace_level")]
        deployment: Option<String>,
        /// Maximum number of results.
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Get a pipeline variable by key.
    Get {
        /// Variable key name.
        #[arg(long)]
        key: String,
        /// Get from workspace-level scope.
        #[arg(long, conflicts_with = "deployment")]
        workspace_level: bool,
        /// Get from deployment environment (name or UUID).
        #[arg(long, conflicts_with = "workspace_level")]
        deployment: Option<String>,
    },
    /// Create a pipeline variable.
    #[command(
        long_about = "Create a pipeline variable.\n\nExamples:\n  bb pipeline var create --key MY_VAR --value hello\n  bb pipeline var create --key SECRET --value s3cr3t --secured\n  bb pipeline var create --workspace-level --key WS_VAR --value val"
    )]
    Create {
        /// Variable key name.
        #[arg(long)]
        key: String,
        /// Variable value.
        #[arg(long)]
        value: String,
        /// Mark variable as secured (write-only, value hidden on read).
        #[arg(long)]
        secured: bool,
        /// Create in workspace-level scope.
        #[arg(long, conflicts_with = "deployment")]
        workspace_level: bool,
        /// Create in deployment environment (name or UUID).
        #[arg(long, conflicts_with = "workspace_level")]
        deployment: Option<String>,
    },
    /// Update a pipeline variable.
    #[command(
        long_about = "Update a pipeline variable.\n\nExamples:\n  bb pipeline var update --key MY_VAR --value newval\n  bb pipeline var update --key MY_VAR --value s3cr3t --secured\n  bb pipeline var update --key MY_VAR --value plaintext --unsecured"
    )]
    Update {
        /// Variable key name.
        #[arg(long)]
        key: String,
        /// New variable value.
        #[arg(long)]
        value: String,
        /// Mark variable as secured.
        #[arg(long, conflicts_with = "unsecured")]
        secured: bool,
        /// Mark variable as unsecured (removes write-only protection).
        #[arg(long, conflicts_with = "secured")]
        unsecured: bool,
        /// Update in workspace-level scope.
        #[arg(long, conflicts_with = "deployment")]
        workspace_level: bool,
        /// Update in deployment environment (name or UUID).
        #[arg(long, conflicts_with = "workspace_level")]
        deployment: Option<String>,
    },
    /// Delete a pipeline variable.
    Delete {
        /// Variable key name.
        #[arg(long)]
        key: String,
        /// Delete from workspace-level scope.
        #[arg(long, conflicts_with = "deployment")]
        workspace_level: bool,
        /// Delete from deployment environment (name or UUID).
        #[arg(long, conflicts_with = "workspace_level")]
        deployment: Option<String>,
        /// Skip confirmation prompt.
        #[arg(long)]
        force: bool,
    },
}

impl VarCommands {
    fn is_workspace_level(&self) -> bool {
        match self {
            VarCommands::List {
                workspace_level, ..
            }
            | VarCommands::Get {
                workspace_level, ..
            }
            | VarCommands::Create {
                workspace_level, ..
            }
            | VarCommands::Update {
                workspace_level, ..
            }
            | VarCommands::Delete {
                workspace_level, ..
            } => *workspace_level,
        }
    }

    fn deployment(&self) -> Option<String> {
        match self {
            VarCommands::List { deployment, .. }
            | VarCommands::Get { deployment, .. }
            | VarCommands::Create { deployment, .. }
            | VarCommands::Update { deployment, .. }
            | VarCommands::Delete { deployment, .. } => deployment.clone(),
        }
    }
}

#[derive(Subcommand, Debug, Clone)]
enum EnvCommands {
    /// List deployment environments.
    List {
        /// Maximum number of results.
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum WebhookCommands {
    /// List webhooks.
    List {
        /// Repository slug.
        repo: String,
    },
    /// Create webhook.
    Create {
        /// Repository slug.
        repo: String,
        /// Webhook URL.
        #[arg(long)]
        url: String,
        /// Description.
        #[arg(long)]
        description: Option<String>,
        /// Events (comma-separated).
        #[arg(long, value_delimiter = ',')]
        events: Vec<String>,
        /// Active flag.
        #[arg(long, default_value_t = true)]
        active: bool,
    },
    /// Delete webhook.
    Delete {
        /// Repository slug.
        repo: String,
        /// Webhook UUID.
        uuid: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum SshKeyCommands {
    /// List SSH deploy keys.
    List {
        /// Repository slug.
        repo: String,
    },
    /// Add SSH deploy key.
    Add {
        /// Repository slug.
        repo: String,
        /// Key label.
        #[arg(long)]
        label: String,
        /// SSH public key.
        #[arg(long)]
        key: String,
    },
    /// Delete SSH deploy key.
    Delete {
        /// Repository slug.
        repo: String,
        /// Key UUID.
        uuid: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum PermissionCommands {
    /// List repository permissions.
    List {
        /// Repository slug.
        repo: String,
    },
    /// Grant repository permission.
    Grant {
        /// Repository slug.
        repo: String,
        /// User UUID.
        #[arg(long)]
        user_uuid: String,
        /// Permission level (read, write, admin).
        #[arg(long)]
        permission: String,
    },
    /// Revoke repository permission.
    Revoke {
        /// Repository slug.
        repo: String,
        /// User UUID.
        user_uuid: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum CommitCommands {
    /// List commits.
    List {
        /// Repository slug.
        repo: String,
        /// Branch name.
        #[arg(long)]
        branch: Option<String>,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get commit details.
    Get {
        /// Repository slug.
        repo: String,
        /// Commit hash.
        hash: String,
    },
    /// View commit diff.
    Diff {
        /// Repository slug.
        repo: String,
        /// Commit hash.
        hash: String,
    },
    /// Browse source code.
    Browse {
        /// Repository slug.
        repo: String,
        /// Commit hash or branch name.
        #[arg(long)]
        commit: String,
        /// File path.
        #[arg(long)]
        path: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum BulkCommands {
    /// Archive stale repositories.
    ArchiveRepos {
        /// Days threshold for staleness.
        #[arg(long, default_value_t = 180)]
        days: i64,
        /// Dry run mode.
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete merged branches.
    DeleteBranches {
        /// Repository slug.
        repo: String,
        /// Exclude patterns (comma-separated).
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,
        /// Dry run mode.
        #[arg(long)]
        dry_run: bool,
    },
}

pub async fn execute(
    args: BitbucketArgs,
    client: ApiClient,
    renderer: &OutputRenderer,
    inferred_workspace: Option<&str>,
    is_bearer: bool,
) -> Result<()> {
    // Whoami doesn't require workspace
    if matches!(args.command, BitbucketCommands::Whoami) {
        return workspaces::whoami(&client, is_bearer).await;
    }

    // Detect git context for auto-detection
    let git_ctx = git::detect_git_context();

    // CLI flag takes precedence, then git context, then inferred from profile
    let workspace = args
        .workspace
        .as_deref()
        .or(git_ctx.workspace.as_deref())
        .or(inferred_workspace)
        .ok_or_else(|| {
            // Build detailed error message with git context
            let mut error_msg = String::from("Workspace required.\n\n");

            // Add current directory
            error_msg.push_str(&format!(
                "Current directory: {}\n",
                git::get_current_directory()
            ));

            // Add git branch or commit SHA
            if let Some(branch) = git::detect_current_branch() {
                error_msg.push_str(&format!("Detected branch: {}\n", branch));
            } else if let Some(sha) = git::get_current_commit_sha() {
                error_msg.push_str(&format!("Git status: Detached HEAD at {}\n", sha));
            }

            error_msg.push_str("\nOptions:\n");
            error_msg.push_str("  1. Use --workspace flag\n");
            error_msg.push_str("  2. Run in git directory with Bitbucket remote\n");
            error_msg.push_str("  3. Configure workspace in profile\n");

            // Show git remotes if available
            let remotes = git::get_all_remotes();
            if !remotes.is_empty() {
                error_msg.push_str("\nFound git remotes:\n");
                for (name, url) in &remotes {
                    error_msg.push_str(&format!("  {} -> {}\n", name, url));
                    // Try to parse workspace/repo from URL
                    if let Some((ws, repo)) = git::parse_git_remote(url) {
                        error_msg.push_str(&format!("    (workspace: {}, repo: {})\n", ws, repo));
                    } else {
                        error_msg.push_str("    (not a Bitbucket remote)\n");
                    }
                }
                error_msg.push_str("\nExample:\n");
                error_msg.push_str(
                    "  atlassian-cli bb pipeline list --workspace <workspace> --repo <repo>\n",
                );
            }

            anyhow::anyhow!(error_msg)
        })?
        .to_string();

    // Global repo slug resolution
    let global_repo = args
        .repo
        .as_deref()
        .or(git_ctx.repo_slug.as_deref())
        .map(|s| s.to_string());

    let ctx = BitbucketContext {
        client,
        renderer,
        is_bearer,
    };

    match args.command {
        BitbucketCommands::Repo(cmd) => match cmd {
            RepoCommands::List { limit } => repos::list_repos(&ctx, &workspace, limit).await,
            RepoCommands::Get { slug } => repos::get_repo(&ctx, &workspace, &slug).await,
            RepoCommands::Create {
                slug,
                name,
                description,
                private,
                project,
            } => {
                repos::create_repo(
                    &ctx,
                    &workspace,
                    &slug,
                    name.as_deref(),
                    description.as_deref(),
                    private,
                    project.as_deref(),
                )
                .await
            }
            RepoCommands::Update {
                slug,
                name,
                description,
                language,
            } => {
                repos::update_repo(
                    &ctx,
                    &workspace,
                    &slug,
                    name.as_deref(),
                    description.as_deref(),
                    language.as_deref(),
                )
                .await
            }
            RepoCommands::Delete { slug, force } => {
                repos::delete_repo(&ctx, &workspace, &slug, force).await
            }
        },
        BitbucketCommands::Branch(cmd) => match cmd {
            BranchCommands::List { repo, limit } => {
                branches::list_branches(&ctx, &workspace, &repo, limit).await
            }
            BranchCommands::Get { repo, branch } => {
                branches::get_branch(&ctx, &workspace, &repo, &branch).await
            }
            BranchCommands::Create { repo, branch, from } => {
                branches::create_branch(&ctx, &workspace, &repo, &branch, &from).await
            }
            BranchCommands::Delete {
                repo,
                branch,
                force,
            } => branches::delete_branch(&ctx, &workspace, &repo, &branch, force).await,
            BranchCommands::Protect {
                repo,
                pattern,
                kind,
                approvals,
            } => {
                branches::protect_branch(&ctx, &workspace, &repo, &pattern, &kind, approvals).await
            }
            BranchCommands::Unprotect {
                repo,
                restriction_id,
            } => branches::unprotect_branch(&ctx, &workspace, &repo, restriction_id).await,
            BranchCommands::Restrictions { repo } => {
                branches::list_restrictions(&ctx, &workspace, &repo).await
            }
        },
        BitbucketCommands::Pr(cmd) => match cmd {
            PrCommands::List { repo, state, limit } => {
                pullrequests::list_pull_requests(&ctx, &workspace, &repo, &state, limit).await
            }
            PrCommands::Get { repo, pr_id } => {
                pullrequests::get_pull_request(&ctx, &workspace, &repo, pr_id).await
            }
            PrCommands::Create {
                repo,
                title,
                source,
                destination,
                description,
                reviewers,
            } => {
                pullrequests::create_pull_request(
                    &ctx,
                    &workspace,
                    &repo,
                    &title,
                    &source,
                    &destination,
                    description.as_deref(),
                    reviewers,
                )
                .await
            }
            PrCommands::Update {
                repo,
                pr_id,
                title,
                description,
            } => {
                pullrequests::update_pull_request(
                    &ctx,
                    &workspace,
                    &repo,
                    pr_id,
                    title.as_deref(),
                    description.as_deref(),
                )
                .await
            }
            PrCommands::Merge {
                repo,
                pr_id,
                strategy,
                message,
            } => {
                pullrequests::merge_pull_request(
                    &ctx,
                    &workspace,
                    &repo,
                    pr_id,
                    strategy.as_deref(),
                    message.as_deref(),
                )
                .await
            }
            PrCommands::Decline { repo, pr_id } => {
                pullrequests::decline_pull_request(&ctx, &workspace, &repo, pr_id).await
            }
            PrCommands::Approve { repo, pr_id } => {
                pullrequests::approve_pull_request(&ctx, &workspace, &repo, pr_id).await
            }
            PrCommands::Unapprove { repo, pr_id } => {
                pullrequests::unapprove_pull_request(&ctx, &workspace, &repo, pr_id).await
            }
            PrCommands::Diff { repo, pr_id } => {
                pullrequests::get_pr_diff(&ctx, &workspace, &repo, pr_id).await
            }
            PrCommands::Comments { repo, pr_id } => {
                pullrequests::list_pr_comments(&ctx, &workspace, &repo, pr_id).await
            }
            PrCommands::Comment { repo, pr_id, text } => {
                pullrequests::add_pr_comment(&ctx, &workspace, &repo, pr_id, &text).await
            }
            PrCommands::Reviewers { repo, pr_id, add } => {
                pullrequests::add_pr_reviewers(&ctx, &workspace, &repo, pr_id, add).await
            }
        },
        BitbucketCommands::Workspace(cmd) => match cmd {
            WorkspaceCommands::List { limit } => workspaces::list_workspaces(&ctx, limit).await,
            WorkspaceCommands::Get { slug } => workspaces::get_workspace(&ctx, &slug).await,
        },
        BitbucketCommands::Project(cmd) => match cmd {
            ProjectCommands::List { limit } => {
                workspaces::list_projects(&ctx, &workspace, limit).await
            }
            ProjectCommands::Get { key } => workspaces::get_project(&ctx, &workspace, &key).await,
            ProjectCommands::Create {
                key,
                name,
                description,
                private,
            } => {
                workspaces::create_project(
                    &ctx,
                    &workspace,
                    &key,
                    &name,
                    description.as_deref(),
                    private,
                )
                .await
            }
            ProjectCommands::Update {
                key,
                name,
                description,
            } => {
                workspaces::update_project(
                    &ctx,
                    &workspace,
                    &key,
                    name.as_deref(),
                    description.as_deref(),
                )
                .await
            }
            ProjectCommands::Delete { key, force } => {
                workspaces::delete_project(&ctx, &workspace, &key, force).await
            }
        },
        BitbucketCommands::Pipeline(cmd) => match cmd {
            PipelineCommands::List {
                limit,
                sort,
                recent,
                branch,
                since,
                before,
                pr,
                all,
                steps,
            } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline list")?;

                // Handle --pr flag
                let effective_branch = if let Some(pr_id) = pr {
                    let pr_info =
                        pullrequests::get_pr_info(&ctx, &workspace, &repo_slug, pr_id).await?;

                    // Check if PR is from a fork
                    if pullrequests::is_from_fork(&pr_info, &workspace, &repo_slug) {
                        eprintln!(
                            "Warning: PR #{} is from a fork (workspace: {}/{})",
                            pr_id, pr_info.source_workspace, pr_info.source_repo
                        );
                        eprintln!("Showing pipelines from the source repository's branch");
                    }

                    // Warn if PR is closed
                    if matches!(
                        pr_info.state.to_uppercase().as_str(),
                        "DECLINED" | "SUPERSEDED"
                    ) {
                        eprintln!("Warning: PR #{} is {}", pr_id, pr_info.state);
                        eprintln!("Showing pipelines for branch: {}", pr_info.source_branch);
                    }

                    // --pr takes precedence over --branch
                    if branch.is_some() {
                        eprintln!(
                            "Warning: Both --pr and --branch specified, using PR source branch"
                        );
                    }

                    Some(pr_info.source_branch)
                } else {
                    branch.clone()
                };

                // Parse time expressions if provided
                let parsed_since = if let Some(s) = since {
                    Some(time_parser::parse_time_expression(&s)?)
                } else {
                    None
                };

                let parsed_before = if let Some(b) = before {
                    Some(time_parser::parse_time_expression(&b)?)
                } else {
                    None
                };

                pipelines::list_pipelines(
                    &ctx,
                    &workspace,
                    &repo_slug,
                    limit,
                    sort.as_deref(),
                    recent,
                    effective_branch.as_deref(),
                    parsed_since.as_deref(),
                    parsed_before.as_deref(),
                    all,
                    steps,
                )
                .await
            }
            PipelineCommands::Get { pipeline_id, steps } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline get")?;
                pipelines::get_pipeline(&ctx, &workspace, &repo_slug, &pipeline_id, steps).await
            }
            PipelineCommands::Latest { branch, steps } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline latest")?;

                // Prefer explicit --branch over auto-detected
                let effective_branch = if let Some(b) = branch {
                    Some(b.clone())
                } else {
                    git::detect_current_branch()
                };

                let branch = effective_branch.ok_or_else(|| {
                    let mut error_msg =
                        String::from("Cannot detect branch for latest pipeline.\n\n");
                    error_msg.push_str(&format!(
                        "Current directory: {}\n",
                        git::get_current_directory()
                    ));
                    if let Some(sha) = git::get_current_commit_sha() {
                        error_msg.push_str(&format!("Git status: Detached HEAD at {}\n", sha));
                    }
                    error_msg.push_str("\nOptions:\n");
                    error_msg.push_str("  1. Use --branch flag to specify branch\n");
                    error_msg.push_str("  2. Checkout a branch: git checkout main\n");
                    error_msg.push_str("\nExample:\n");
                    error_msg.push_str("  atlassian-cli bb pipeline latest --branch main\n");
                    anyhow::anyhow!(error_msg)
                })?;

                // Call list_pipelines with limit=1 to get latest
                pipelines::list_pipelines(
                    &ctx,
                    &workspace,
                    &repo_slug,
                    1,                     // limit
                    Some("-created_on"),   // sort
                    None,                  // recent
                    Some(branch.as_str()), // branch filter
                    None,                  // since
                    None,                  // before
                    false,                 // all
                    steps,                 // steps
                )
                .await
            }
            PipelineCommands::Trigger {
                ref_name,
                ref_type,
                variables,
                secured,
            } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline trigger")?;
                pipelines::trigger_pipeline(
                    &ctx, &workspace, &repo_slug, &ref_name, &ref_type, variables, secured,
                )
                .await
            }
            PipelineCommands::Stop { pipeline_id } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline stop")?;
                // Resolve build number to UUID if needed
                let pipeline_uuid =
                    pipelines::resolve_pipeline_id(&ctx, &workspace, &repo_slug, &pipeline_id)
                        .await?;
                pipelines::stop_pipeline(&ctx, &workspace, &repo_slug, &pipeline_uuid).await
            }
            PipelineCommands::Logs {
                pipeline_id,
                step_uuid,
                step,
                grep,
                ignore_case,
                failed_only,
            } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline logs")?;
                // Resolve build number to UUID if needed
                let pipeline_uuid =
                    pipelines::resolve_pipeline_id(&ctx, &workspace, &repo_slug, &pipeline_id)
                        .await?;
                pipelines::get_pipeline_logs(
                    &ctx,
                    &workspace,
                    &repo_slug,
                    &pipeline_uuid,
                    step_uuid.as_deref(),
                    step.as_deref(),
                    grep.as_deref(),
                    ignore_case,
                    failed_only,
                )
                .await
            }
            PipelineCommands::Watch {
                pipeline_id,
                interval,
                steps,
            } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline watch")?;
                pipelines::watch_pipeline(
                    &ctx,
                    &workspace,
                    &repo_slug,
                    &pipeline_id,
                    interval,
                    steps,
                )
                .await
            }
            PipelineCommands::Steps { pipeline_id } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline steps")?;
                pipelines::list_steps(&ctx, &workspace, &repo_slug, &pipeline_id).await
            }
            PipelineCommands::Status { steps } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline status")?;
                pipelines::pipeline_status(&ctx, &workspace, &repo_slug, steps).await
            }
            PipelineCommands::Rerun {
                pipeline_id,
                pr,
                failed_only,
                variables,
                secured,
            } => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline rerun")?;

                // Determine which pipeline to rerun
                let effective_pipeline_id = if let Some(pr_id) = pr {
                    // Handle --pr flag
                    let pr_info =
                        pullrequests::get_pr_info(&ctx, &workspace, &repo_slug, pr_id).await?;

                    // Error if PR is from a fork
                    if pullrequests::is_from_fork(&pr_info, &workspace, &repo_slug) {
                        anyhow::bail!(
                            "Error: PR #{} is from a fork (workspace: {}/{})\n\n\
                            Cannot access pipelines from forked repositories.\n\
                            To rerun, use --branch with the source branch name:\n  \
                            atlassian-cli bb pipeline rerun --branch {}",
                            pr_id,
                            pr_info.source_workspace,
                            pr_info.source_repo,
                            pr_info.source_branch
                        );
                    }

                    // Find latest pipeline for PR branch
                    let latest = pipelines::find_latest_pipeline_for_branch(
                        &ctx,
                        &workspace,
                        &repo_slug,
                        &pr_info.source_branch,
                    )
                    .await?;

                    // If --failed-only, check for failed steps
                    if failed_only {
                        let has_failed = pipelines::pipeline_has_failed_steps(
                            &ctx, &workspace, &repo_slug, &latest,
                        )
                        .await?;

                        if !has_failed {
                            println!(
                                "Skipping rerun: No failed steps in latest pipeline for PR #{}",
                                pr_id
                            );
                            return Ok(());
                        }
                    }

                    latest
                } else if let Some(id) = pipeline_id {
                    id.clone()
                } else {
                    anyhow::bail!("Either --pr or pipeline_id is required");
                };

                pipelines::rerun_pipeline(
                    &ctx,
                    &workspace,
                    &repo_slug,
                    &effective_pipeline_id,
                    variables,
                    secured,
                )
                .await
            }
            PipelineCommands::Var(var_cmd) => {
                let scope = if var_cmd.is_workspace_level() {
                    variables::VarScope::Workspace {
                        workspace: workspace.clone(),
                    }
                } else if let Some(env_name) = var_cmd.deployment() {
                    let repo_slug =
                        require_repo(None, global_repo.as_deref(), "pipeline var (deployment)")?;
                    let env_uuid = variables::resolve_environment_uuid(
                        &ctx, &workspace, &repo_slug, &env_name,
                    )
                    .await?;
                    variables::VarScope::Deployment {
                        workspace: workspace.clone(),
                        repo_slug,
                        env_uuid,
                    }
                } else {
                    let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline var")?;
                    variables::VarScope::Repository {
                        workspace: workspace.clone(),
                        repo_slug,
                    }
                };

                match var_cmd {
                    VarCommands::List { limit, .. } => {
                        variables::list_variables(&ctx, &scope, limit).await
                    }
                    VarCommands::Get { key, .. } => {
                        variables::get_variable(&ctx, &scope, &key).await
                    }
                    VarCommands::Create {
                        key,
                        value,
                        secured,
                        ..
                    } => variables::create_variable(&ctx, &scope, &key, &value, secured).await,
                    VarCommands::Update {
                        key,
                        value,
                        secured,
                        unsecured,
                        ..
                    } => {
                        let secured_opt = if secured {
                            Some(true)
                        } else if unsecured {
                            Some(false)
                        } else {
                            None
                        };
                        variables::update_variable(&ctx, &scope, &key, &value, secured_opt).await
                    }
                    VarCommands::Delete { key, force, .. } => {
                        if !force {
                            eprintln!("Warning: This will permanently delete variable '{}'.", key);
                            eprintln!("Use --force to skip this check.");
                            anyhow::bail!(
                                "Refusing to delete without --force. Use --force to confirm."
                            );
                        }
                        variables::delete_variable(&ctx, &scope, &key).await
                    }
                }
            }
            PipelineCommands::Env(env_cmd) => {
                let repo_slug = require_repo(None, global_repo.as_deref(), "pipeline env")?;
                match env_cmd {
                    EnvCommands::List { limit } => {
                        variables::list_environments(&ctx, &workspace, &repo_slug, limit).await
                    }
                }
            }
        },
        BitbucketCommands::Webhook(cmd) => match cmd {
            WebhookCommands::List { repo } => {
                webhooks::list_webhooks(&ctx, &workspace, &repo).await
            }
            WebhookCommands::Create {
                repo,
                url,
                description,
                events,
                active,
            } => {
                webhooks::create_webhook(
                    &ctx,
                    &workspace,
                    &repo,
                    &url,
                    description.as_deref(),
                    events,
                    active,
                )
                .await
            }
            WebhookCommands::Delete { repo, uuid } => {
                webhooks::delete_webhook(&ctx, &workspace, &repo, &uuid).await
            }
        },
        BitbucketCommands::SshKey(cmd) => match cmd {
            SshKeyCommands::List { repo } => webhooks::list_ssh_keys(&ctx, &workspace, &repo).await,
            SshKeyCommands::Add { repo, label, key } => {
                webhooks::add_ssh_key(&ctx, &workspace, &repo, &label, &key).await
            }
            SshKeyCommands::Delete { repo, uuid } => {
                webhooks::delete_ssh_key(&ctx, &workspace, &repo, &uuid).await
            }
        },
        BitbucketCommands::Permission(cmd) => match cmd {
            PermissionCommands::List { repo } => {
                permissions::list_repo_permissions(&ctx, &workspace, &repo).await
            }
            PermissionCommands::Grant {
                repo,
                user_uuid,
                permission,
            } => {
                permissions::grant_repo_permission(&ctx, &workspace, &repo, &user_uuid, &permission)
                    .await
            }
            PermissionCommands::Revoke { repo, user_uuid } => {
                permissions::revoke_repo_permission(&ctx, &workspace, &repo, &user_uuid).await
            }
        },
        BitbucketCommands::Commit(cmd) => match cmd {
            CommitCommands::List {
                repo,
                branch,
                limit,
            } => commits::list_commits(&ctx, &workspace, &repo, branch.as_deref(), limit).await,
            CommitCommands::Get { repo, hash } => {
                commits::get_commit(&ctx, &workspace, &repo, &hash).await
            }
            CommitCommands::Diff { repo, hash } => {
                commits::get_commit_diff(&ctx, &workspace, &repo, &hash).await
            }
            CommitCommands::Browse { repo, commit, path } => {
                commits::browse_source(&ctx, &workspace, &repo, &commit, path.as_deref()).await
            }
        },
        BitbucketCommands::Bulk(cmd) => match cmd {
            BulkCommands::ArchiveRepos { days, dry_run } => {
                bulk::archive_stale_repos(&ctx, &workspace, days, dry_run).await
            }
            BulkCommands::DeleteBranches {
                repo,
                exclude,
                dry_run,
            } => bulk::delete_merged_branches(&ctx, &workspace, &repo, exclude, dry_run).await,
        },
        BitbucketCommands::Whoami => unreachable!("handled above"),
    }
}
