use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};

// Submodules
mod branches;
mod bulk;
mod commits;
mod permissions;
mod pipelines;
mod pullrequests;
mod repos;
pub mod utils;
mod webhooks;
mod workspaces;

use utils::BitbucketContext;

#[derive(Args, Debug, Clone)]
pub struct BitbucketArgs {
    /// Workspace slug (defaults to workspace configured in profile base URL host prefix).
    #[arg(long)]
    workspace: Option<String>,

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
}

#[derive(Subcommand, Debug, Clone)]
enum RepoCommands {
    /// List repositories inside a workspace.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Show repository metadata.
    Get { slug: String },
    /// Create a new repository.
    Create {
        /// Repository slug (URL-friendly name).
        slug: String,
        /// Repository display name.
        #[arg(long)]
        name: Option<String>,
        /// Repository description.
        #[arg(long)]
        description: Option<String>,
        /// Make repository private.
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
        /// Repository slug.
        repo: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get pipeline details.
    Get {
        /// Repository slug.
        repo: String,
        /// Pipeline UUID.
        uuid: String,
    },
    /// Trigger a new pipeline.
    Trigger {
        /// Repository slug.
        repo: String,
        /// Branch or tag name.
        #[arg(long)]
        ref_name: String,
        /// Reference type (branch or tag).
        #[arg(long, default_value = "branch")]
        ref_type: String,
    },
    /// Stop a running pipeline.
    Stop {
        /// Repository slug.
        repo: String,
        /// Pipeline UUID.
        uuid: String,
    },
    /// Get pipeline logs.
    Logs {
        /// Repository slug.
        repo: String,
        /// Pipeline UUID.
        pipeline_uuid: String,
        /// Step UUID.
        step_uuid: String,
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
) -> Result<()> {
    let workspace = args.workspace.ok_or_else(|| {
        anyhow::anyhow!("Workspace must be provided (--workspace) or inferred from base URL")
    })?;

    let ctx = BitbucketContext { client, renderer };

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
            PipelineCommands::List { repo, limit } => {
                pipelines::list_pipelines(&ctx, &workspace, &repo, limit).await
            }
            PipelineCommands::Get { repo, uuid } => {
                pipelines::get_pipeline(&ctx, &workspace, &repo, &uuid).await
            }
            PipelineCommands::Trigger {
                repo,
                ref_name,
                ref_type,
            } => pipelines::trigger_pipeline(&ctx, &workspace, &repo, &ref_name, &ref_type).await,
            PipelineCommands::Stop { repo, uuid } => {
                pipelines::stop_pipeline(&ctx, &workspace, &repo, &uuid).await
            }
            PipelineCommands::Logs {
                repo,
                pipeline_uuid,
                step_uuid,
            } => {
                pipelines::get_pipeline_logs(&ctx, &workspace, &repo, &pipeline_uuid, &step_uuid)
                    .await
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
    }
}
