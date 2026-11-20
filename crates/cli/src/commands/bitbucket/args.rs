//! The clap argument tree of the Bitbucket commands.
//!
//! `mod.rs` owns the entry point and the dispatch. This module owns the
//! shape of each command line.

use clap::Subcommand;

use super::pullrequests;

#[derive(Subcommand, Debug, Clone)]
pub(super) enum BitbucketCommands {
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

    /// Call any Bitbucket REST endpoint with the profile's credentials.
    ///
    /// The escape hatch for anything the typed commands do not expose. Paths
    /// are relative to https://api.bitbucket.org, e.g. /2.0/user.
    #[command(
        long_about = "Call any Bitbucket REST endpoint with the profile's credentials.\n\nThe escape hatch for fields the typed commands drop. Paths are relative to https://api.bitbucket.org.\n\nExamples:\n  bb api /2.0/user\n  bb api /2.0/repositories/{workspace}/{repo}/pullrequests/1\n  bb api /2.0/repositories/{workspace}/{repo}/default-reviewers"
    )]
    Api(crate::commands::api::ApiArgs),
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum RepoCommands {
    /// List repositories inside a workspace.
    #[command(
        long_about = "List repositories inside a workspace.\n\nExamples:\n  bb repo list\n  bb repo list --limit 50\n  bb repo list --workspace my-team"
    )]
    List {
        /// Maximum results. 0 fetches every page.
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Show repository metadata.
    #[command(long_about = "Show repository metadata.\n\nExamples:\n  bb repo get my-repo")]
    Get {
        /// Repository slug (e.g., my-repo). Defaults to --repo, then the git remote.
        slug: Option<String>,
    },
    /// Create a new repository.
    #[command(
        long_about = "Create a new repository.\n\nExamples:\n  bb repo create my-new-repo\n  bb repo create my-repo --name \"My Repository\" --description \"Project repo\" --private"
    )]
    Create {
        /// Repository slug (URL-friendly name, e.g., my-repo), or use --repo.
        /// Not read from the git remote, because this command names a new
        /// repository.
        slug: Option<String>,
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
        /// Repository slug, or use --repo. Not read from the git remote, because
        /// this command overwrites repository settings.
        slug: Option<String>,
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
    /// Delete a repository, its pull requests, issues and wiki.
    Delete {
        /// Repository slug, or use --repo. Not read from the git remote, because
        /// this command deletes.
        slug: Option<String>,
        /// Skip the typed confirmation. `--yes` matches the other commands.
        #[arg(long, visible_alias = "yes")]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum BranchCommands {
    /// List branches in a repository.
    List {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// Maximum results. 0 fetches every page.
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get branch details.
    #[command(override_usage = "atlassian-cli bitbucket branch get [OPTIONS] [REPO] <BRANCH>")]
    Get {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// branch name.
        #[arg(value_names = ["REPO", "BRANCH"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// Create a new branch.
    #[command(
        override_usage = "atlassian-cli bitbucket branch create [OPTIONS] --from <FROM> [REPO] <BRANCH>"
    )]
    Create {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// new branch name.
        #[arg(value_names = ["REPO", "BRANCH"], num_args = 1..=2)]
        targets: Vec<String>,
        /// Source commit hash or branch name.
        #[arg(long)]
        from: String,
    },
    /// Delete a branch.
    #[command(override_usage = "atlassian-cli bitbucket branch delete [OPTIONS] [REPO] <BRANCH>")]
    Delete {
        /// Repository slug (or use --repo; not read from the git remote, because
        /// this command deletes), then the branch name.
        #[arg(value_names = ["REPO", "BRANCH"], num_args = 1..=2)]
        targets: Vec<String>,
        /// Skip the typed confirmation. `--yes` matches the bulk commands.
        #[arg(long, visible_alias = "yes")]
        force: bool,
    },
    /// Add branch protection (restriction).
    Protect {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
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
    #[command(
        override_usage = "atlassian-cli bitbucket branch unprotect [OPTIONS] [REPO] <RESTRICTION_ID>"
    )]
    Unprotect {
        /// Repository slug (or use --repo; not read from the git remote, because
        /// this command deletes), then the branch restriction ID.
        #[arg(value_names = ["REPO", "RESTRICTION_ID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// List branch restrictions.
    Restrictions {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
    },
}

/// `clap` value parser that trims and rejects empty strings.
///
/// Used where an empty value would only produce a server-side 400 (`--text ""`,
/// `--path ""`). Bitbucket rejects both, and the error is far more useful
/// raised by clap at parse time.
pub(super) fn non_empty_string(s: &str) -> std::result::Result<String, String> {
    if s.trim().is_empty() {
        Err("value must not be empty".to_string())
    } else {
        Ok(s.to_string())
    }
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum PrCommands {
    /// List pull requests for a repository.
    List {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        #[arg(long, default_value = "OPEN")]
        state: String,
        /// Maximum results. 0 fetches every page.
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get pull request details.
    #[command(override_usage = "atlassian-cli bitbucket pr get [OPTIONS] [REPO] <PR_ID>")]
    Get {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// Create a new pull request.
    Create {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
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
        /// Reviewer UUIDs (comma-separated). `bb permission list` and
        /// `bb pr reviewers` both report the uuid to use.
        #[arg(long, value_delimiter = ',')]
        reviewers: Vec<String>,
        /// Also add the repository's configured default reviewers.
        ///
        /// Atlassian apply these in the web UI only, so a pull request created
        /// through the API gets none. Where a merge check requires a
        /// default-reviewer approval, that leaves the pull request unmergeable.
        #[arg(long)]
        default_reviewers: bool,
    },
    /// Update pull request.
    #[command(override_usage = "atlassian-cli bitbucket pr update [OPTIONS] [REPO] <PR_ID>")]
    Update {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// Replace the reviewers with these account UUIDs (comma-separated).
        /// An empty value removes every reviewer. `pr reviewers --add` adds
        /// to the current set instead.
        #[arg(long, value_delimiter = ',')]
        reviewers: Option<Vec<String>>,
    },
    /// Merge pull request.
    #[command(override_usage = "atlassian-cli bitbucket pr merge [OPTIONS] [REPO] <PR_ID>")]
    Merge {
        /// Repository slug (or use --repo; not read from the git remote, because
        /// a merge is not reversible), then the pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
        /// Merge strategy: merge_commit, squash, or fast_forward.
        #[arg(long)]
        strategy: Option<String>,
        /// Merge commit message.
        #[arg(long)]
        message: Option<String>,
    },
    /// Decline/close pull request.
    #[command(override_usage = "atlassian-cli bitbucket pr decline [OPTIONS] [REPO] <PR_ID>")]
    Decline {
        /// Repository slug (or use --repo; not read from the git remote, because
        /// a declined pull request cannot be reopened), then the pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// Approve pull request.
    #[command(override_usage = "atlassian-cli bitbucket pr approve [OPTIONS] [REPO] <PR_ID>")]
    Approve {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// Remove approval from pull request.
    #[command(override_usage = "atlassian-cli bitbucket pr unapprove [OPTIONS] [REPO] <PR_ID>")]
    Unapprove {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// View pull request diff.
    #[command(override_usage = "atlassian-cli bitbucket pr diff [OPTIONS] [REPO] <PR_ID>")]
    Diff {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// List pull request comments.
    #[command(override_usage = "atlassian-cli bitbucket pr comments [OPTIONS] [REPO] <PR_ID>")]
    Comments {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// Add a comment to a pull request, optionally inline or as a threaded reply
    #[command(long_about = "Add a comment to a pull request.\n\n\
        By default, posts a top-level PR comment. Passing --path (with an optional --line and\n\
        --side) anchors it to a file instead, and --parent replies inside an existing thread.\n\n\
        Examples:\n  \
        bb pr comment my-repo 123 --text \"LGTM\"\n  \
        bb pr comment my-repo 123 --text \"Nit: rename\" --path src/main.rs --line 42\n  \
        bb pr comment my-repo 123 --text \"Why remove?\" --path src/main.rs --line 17 --side old\n  \
        bb pr comment my-repo 123 --text \"Whole-file comment\" --path README.md\n  \
        bb pr comment my-repo 123 --text \"Fixed\" --parent 843649259\n\n\
        --side selects which side of the diff --line refers to:\n  \
        new (default) = destination revision (added/unchanged lines)\n  \
        old           = source revision (removed lines)")]
    #[command(
        override_usage = "atlassian-cli bitbucket pr comment [OPTIONS] --text <TEXT> [REPO] <PR_ID>"
    )]
    Comment {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
        /// Comment text (Markdown).
        #[arg(long, value_parser = non_empty_string)]
        text: String,
        /// Parent comment ID for a threaded reply.
        #[arg(long)]
        parent: Option<i64>,
        /// File path (relative to repo root) to anchor an inline comment to.
        /// If omitted, posts a top-level PR comment.
        #[arg(long, value_parser = non_empty_string)]
        path: Option<String>,
        /// Line number to anchor the inline comment to. Must be >= 1, because
        /// Bitbucket line numbers are 1-indexed. Requires --path.
        /// If omitted while --path is given, posts a file-level inline comment.
        #[arg(long, requires = "path", value_parser = clap::value_parser!(u32).range(1..))]
        line: Option<u32>,
        /// Which side of the diff --line refers to: `new` (default) = destination
        /// (added/unchanged lines), `old` = source (removed lines). Requires --line.
        #[arg(long, requires = "line", default_value_t = pullrequests::Side::New, value_enum)]
        side: pullrequests::Side,
    },
    /// Resolve a pull request diff comment thread.
    #[command(
        override_usage = "atlassian-cli bitbucket pr resolve-comment [OPTIONS] [REPO] <PR_ID> <COMMENT_ID>"
    )]
    ResolveComment {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID and the top-level comment ID.
        #[arg(value_names = ["REPO", "PR_ID", "COMMENT_ID"], num_args = 1..=3)]
        targets: Vec<String>,
    },
    /// Reopen a resolved pull request comment thread.
    #[command(
        override_usage = "atlassian-cli bitbucket pr reopen-comment [OPTIONS] [REPO] <PR_ID> <COMMENT_ID>"
    )]
    ReopenComment {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID and the top-level comment ID.
        #[arg(value_names = ["REPO", "PR_ID", "COMMENT_ID"], num_args = 1..=3)]
        targets: Vec<String>,
    },
    /// List pull request reviewers with review status, or add new reviewers.
    #[command(
        long_about = "List pull request reviewers with review status, or add new reviewers.\n\nWith no flags, lists current reviewers (role=REVIEWER) and whether each has approved, requested changes, or not responded.\n\nExamples:\n  bb pr reviewers my-repo 123\n  bb pr reviewers my-repo 123 --all\n  bb pr reviewers my-repo 123 --add {uuid}"
    )]
    #[command(override_usage = "atlassian-cli bitbucket pr reviewers [OPTIONS] [REPO] <PR_ID>")]
    Reviewers {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pull request ID.
        #[arg(value_names = ["REPO", "PR_ID"], num_args = 1..=2)]
        targets: Vec<String>,
        /// Reviewer UUIDs to add (comma-separated). If omitted, lists current reviewers instead.
        #[arg(long, value_delimiter = ',')]
        add: Vec<String>,
        /// When listing, also include non-reviewer participants (commenters).
        #[arg(long, conflicts_with = "add")]
        all: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum WorkspaceCommands {
    /// List workspaces.
    List {
        /// Maximum results. 0 fetches every page.
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get workspace details.
    Get { slug: String },
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum ProjectCommands {
    /// List projects in workspace.
    List {
        /// Maximum results. 0 fetches every page.
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
pub(super) enum PipelineCommands {
    /// List pipelines.
    List {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// Maximum results. 0 fetches every page.
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
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pipeline UUID or build number.
        #[arg(value_names = ["REPO", "PIPELINE"], num_args = 0..=2)]
        targets: Vec<String>,
        /// Pipeline UUID or build number (alternative to positional).
        #[arg(long = "pipeline")]
        pipeline_flag: Option<String>,
        /// Show pipeline steps with status.
        #[arg(long)]
        steps: bool,
    },
    /// Get latest pipeline for current or specified branch.
    Latest {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// Branch name (auto-detected from current branch if not specified).
        #[arg(long)]
        branch: Option<String>,
        /// Show pipeline steps with status.
        #[arg(long)]
        steps: bool,
    },
    /// Trigger a new pipeline.
    Trigger {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
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
        /// Custom pipeline name to trigger (selector pattern from bitbucket-pipelines.yml).
        #[arg(long)]
        custom_pipeline: Option<String>,
    },
    /// Stop a running pipeline.
    Stop {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pipeline UUID or build number.
        #[arg(value_names = ["REPO", "PIPELINE"], num_args = 0..=2)]
        targets: Vec<String>,
        /// Pipeline UUID or build number (alternative to positional).
        #[arg(long = "pipeline")]
        pipeline_flag: Option<String>,
    },
    /// Get pipeline logs.
    Logs {
        /// Repository slug (defaults to --repo, then the git remote), pipeline
        /// UUID or build number, step UUID.
        #[arg(value_names = ["REPO", "PIPELINE", "STEP_UUID"], num_args = 0..=3)]
        targets: Vec<String>,
        /// Pipeline UUID or build number (alternative to positional).
        #[arg(long = "pipeline")]
        pipeline_flag: Option<String>,
        /// Step UUID (alternative to positional).
        #[arg(long = "step-uuid")]
        step_uuid_flag: Option<String>,
        /// Filter by step name pattern.
        #[arg(long)]
        step: Option<String>,
        /// Grep pattern to filter log lines.
        #[arg(long)]
        grep: Option<String>,
        /// Case-insensitive grep and step name matching.
        #[arg(long, short = 'i')]
        ignore_case: bool,
        /// Show only failed steps.
        #[arg(long)]
        failed_only: bool,
    },
    /// Watch a running pipeline until it finishes or pauses for a manual step.
    ///
    /// Exit codes: 0 successful, 1 failed, stopped or unknown, 2 timed out,
    /// 3 paused waiting on a manual step.
    Watch {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pipeline UUID or build number.
        #[arg(value_names = ["REPO", "PIPELINE"], num_args = 0..=2)]
        targets: Vec<String>,
        /// Pipeline UUID or build number (alternative to positional).
        #[arg(long = "pipeline")]
        pipeline_flag: Option<String>,
        /// Poll interval in seconds.
        #[arg(long, default_value_t = 5)]
        interval: u64,
        /// Show pipeline steps with status.
        #[arg(long)]
        steps: bool,
        /// Shell command to run on pipeline completion. Receives env vars: PIPELINE_STATUS, PIPELINE_BUILD_NUMBER, PIPELINE_UUID, PIPELINE_REF_NAME.
        #[arg(long)]
        on_complete: Option<String>,
        /// Timeout in seconds. Exit with code 2 if pipeline does not finish in time.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        timeout: Option<u64>,
        /// Log mode: one line per poll, no ANSI escape codes. Auto-enabled when stdout is not a TTY.
        #[arg(long)]
        log: bool,
    },
    /// List steps for a pipeline.
    Steps {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pipeline UUID or build number.
        #[arg(value_names = ["REPO", "PIPELINE"], num_args = 0..=2)]
        targets: Vec<String>,
        /// Pipeline UUID or build number (alternative to positional).
        #[arg(long = "pipeline")]
        pipeline_flag: Option<String>,
    },
    /// Get latest pipeline status (JSON output, smart exit codes).
    ///
    /// Exit codes: 0 successful, 1 failed, stopped or unknown, 2 in progress or
    /// pending, 3 paused waiting on a manual step. `--wait` stops at a pause.
    Status {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// Show pipeline steps with status.
        #[arg(long)]
        steps: bool,
        /// Wait until the pipeline finishes, or pauses for a manual step.
        #[arg(long)]
        wait: bool,
        /// Poll interval in seconds (used with --wait).
        #[arg(long, default_value_t = 10)]
        interval: u64,
    },
    /// Re-run a pipeline with the same commit.
    Rerun {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// pipeline UUID or build number (omit with --pr).
        #[arg(value_names = ["REPO", "PIPELINE"], num_args = 0..=2)]
        targets: Vec<String>,
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
pub(super) enum VarCommands {
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
    pub(super) fn is_workspace_level(&self) -> bool {
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

    pub(super) fn deployment(&self) -> Option<String> {
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
pub(super) enum EnvCommands {
    /// List deployment environments.
    List {
        /// Maximum number of results.
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum WebhookCommands {
    /// List webhooks.
    List {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
    },
    /// Create webhook.
    Create {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
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
    #[command(override_usage = "atlassian-cli bitbucket webhook delete [OPTIONS] [REPO] <UUID>")]
    Delete {
        /// Repository slug (or use --repo; not read from the git remote, because
        /// this command deletes), then the webhook UUID.
        #[arg(value_names = ["REPO", "UUID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum SshKeyCommands {
    /// List SSH deploy keys.
    List {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
    },
    /// Add SSH deploy key.
    Add {
        /// Repository slug, or use --repo. Not read from the git remote, because
        /// this command gives access.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// Key label.
        #[arg(long)]
        label: String,
        /// SSH public key.
        #[arg(long)]
        key: String,
    },
    /// Delete SSH deploy key.
    #[command(override_usage = "atlassian-cli bitbucket ssh-key delete [OPTIONS] [REPO] <UUID>")]
    Delete {
        /// Repository slug (or use --repo; not read from the git remote, because
        /// this command deletes), then the key UUID.
        #[arg(value_names = ["REPO", "UUID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum PermissionCommands {
    /// List repository permissions.
    List {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
    },
    /// Grant repository permission.
    Grant {
        /// Repository slug, or use --repo. Not read from the git remote, because
        /// this command gives access.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// User UUID.
        #[arg(long)]
        user_uuid: String,
        /// Permission level (read, write, admin).
        #[arg(long)]
        permission: String,
    },
    /// Revoke repository permission.
    #[command(
        override_usage = "atlassian-cli bitbucket permission revoke [OPTIONS] [REPO] <USER_UUID>"
    )]
    Revoke {
        /// Repository slug (or use --repo; not read from the git remote, because
        /// this command deletes), then the user UUID.
        #[arg(value_names = ["REPO", "USER_UUID"], num_args = 1..=2)]
        targets: Vec<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum CommitCommands {
    /// List commits.
    List {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// Branch name.
        #[arg(long)]
        branch: Option<String>,
        /// Maximum results. 0 fetches every page.
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get commit details.
    #[command(override_usage = "atlassian-cli bitbucket commit get [OPTIONS] [REPO] <HASH>")]
    Get {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// commit hash.
        #[arg(value_names = ["REPO", "HASH"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// View commit diff.
    #[command(override_usage = "atlassian-cli bitbucket commit diff [OPTIONS] [REPO] <HASH>")]
    Diff {
        /// Repository slug (defaults to --repo, then the git remote), then the
        /// commit hash.
        #[arg(value_names = ["REPO", "HASH"], num_args = 1..=2)]
        targets: Vec<String>,
    },
    /// List a directory, or print the content of a file.
    ///
    /// With no --path, or with a --path that is a directory, the command lists
    /// the entries. With a --path that is a file, the command writes the file
    /// content to stdout: the bytes are not changed, no newline is added, and
    /// --format has no effect on the content. With --output <file>, the content
    /// goes to the file, and the confirmation on stdout obeys --format.
    #[command(after_help = "EXAMPLES:\n  \
            bb commit browse web_app --commit main                      # list the root\n  \
            bb commit browse web_app --commit main --path src           # list a directory\n  \
            bb commit browse web_app --commit main --path src/main.rs   # print a file\n  \
            bb commit browse web_app --commit 1a2b3c4 --path logo.png --output ./logo.png")]
    Browse {
        /// Repository slug. Defaults to --repo, then the git remote.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// Commit hash, branch or tag name.
        #[arg(long)]
        commit: String,
        /// Path of a directory (lists it) or of a file (prints its content).
        #[arg(long)]
        path: Option<String>,
        /// Write the file content to this file and not to stdout. `-` is
        /// stdout. Applies to a file path only.
        #[arg(long, short = 'o')]
        output: Option<std::path::PathBuf>,
        /// Replace the --output file if it exists.
        #[arg(long, requires = "output")]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(super) enum BulkCommands {
    /// Disable the issue tracker and wiki on stale repositories.
    ///
    /// Despite the command name, this does NOT archive: Bitbucket Cloud has no
    /// repository archive API. It sets has_issues and has_wiki to false, which
    /// makes any existing issues and wiki pages inaccessible. Every repository
    /// in the workspace is examined, not just the first page. Lists candidates
    /// by default; pass --execute to apply.
    ArchiveRepos {
        /// Days threshold for staleness. Must be at least 1.
        ///
        /// Rejecting 0 and negatives matters: either would make every
        /// repository "stale", which with --execute --yes in a script is a
        /// workspace-wide change from a typo.
        #[arg(long, default_value_t = 180, value_parser = clap::value_parser!(i64).range(1..=36_500))]
        days: i64,
        /// Apply the change. Without this, candidates are only listed.
        #[arg(long)]
        execute: bool,
        /// Skip the typed confirmation required by --execute.
        #[arg(long)]
        yes: bool,
        /// Deprecated: listing is now the default. Cannot be combined with
        /// --execute, which it would otherwise silently override.
        #[arg(long, hide = true, conflicts_with = "execute")]
        dry_run: bool,
    },
    /// Delete branches by name. Does NOT check merge status.
    ///
    /// Any branch except main, master, develop, development and --exclude
    /// matches is a candidate, whether or not it was ever merged. EVERY branch
    /// in the repository is considered, not just the first page. Lists
    /// candidates by default; pass --execute to actually delete.
    DeleteBranches {
        /// Repository slug, or use --repo. Not read from the git remote, because
        /// this command deletes.
        #[arg(value_name = "REPO")]
        repo_arg: Option<String>,
        /// Exclude patterns (comma-separated).
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,
        /// Delete the matched branches. Without this, they are only listed.
        #[arg(long)]
        execute: bool,
        /// Skip the typed confirmation required by --execute.
        #[arg(long)]
        yes: bool,
        /// Deprecated: listing is now the default. Cannot be combined with
        /// --execute, which it would otherwise silently override.
        #[arg(long, hide = true, conflicts_with = "execute")]
        dry_run: bool,
    },
}
