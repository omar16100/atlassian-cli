use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::Args;

// Submodules
mod args;
mod branches;
mod bulk;
mod commits;
mod git;
mod permissions;
mod pipelines;
mod positional;
mod pullrequests;
mod repos;
mod time_parser;
pub mod utils;
mod variables;
mod webhooks;
// `auth whoami --bitbucket` reuses the identity reporting here rather than
// reimplementing the bearer/basic split.
pub(crate) mod workspaces;

#[cfg(test)]
mod path_encoding_tests;
use args::*;
use utils::BitbucketContext;

/// Resolve pipeline ID from positional arg or --pipeline flag.
fn resolve_pipeline_arg(
    positional: Option<String>,
    flag: Option<String>,
) -> anyhow::Result<String> {
    positional.or(flag).ok_or_else(|| {
        anyhow::anyhow!("Pipeline ID required. Provide as positional arg or --pipeline <ID>")
    })
}

/// Helper to require a repo slug with a clear error message
fn require_repo(
    explicit: Option<&str>,
    git_detected: Option<&str>,
    cmd: &str,
) -> anyhow::Result<String> {
    explicit.or(git_detected).map(String::from).ok_or_else(|| {
        let mut msg = format!("Repository required for '{cmd}'.\n\n");
        msg.push_str("Not in a git directory with Bitbucket remote.\n\n");

        // Show detected remotes if available
        let remotes = git::get_all_remotes();
        if !remotes.is_empty() {
            msg.push_str("Detected git remotes:\n");
            for (name, url) in &remotes {
                let redacted = git::redact_url(url);
                if let Some((ws, repo)) = git::parse_git_remote(url) {
                    msg.push_str(&format!(
                        "  {name} -> {redacted} (workspace: {ws}, repo: {repo})\n"
                    ));
                } else {
                    msg.push_str(&format!("  {name} -> {redacted} (not a Bitbucket remote)\n"));
                }
            }
            msg.push('\n');
        }

        msg.push_str("Tip: use --workspace <slug> --repo <slug>, or run from a directory with a Bitbucket git remote.\n");
        msg.push_str(&format!(
            "\nExample:\n  atlassian-cli bb {cmd} --workspace <workspace> --repo <repo>\n"
        ));

        anyhow::anyhow!(msg)
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

pub async fn execute(
    args: BitbucketArgs,
    client: ApiClient,
    renderer: &OutputRenderer,
    inferred_workspace: Option<&str>,
    is_bearer: bool,
    bitbucket_remote: Option<&str>,
) -> Result<()> {
    // Whoami doesn't require workspace
    if matches!(args.command, BitbucketCommands::Whoami) {
        return workspaces::whoami(&client, is_bearer, renderer, None).await;
    }

    // Neither does the raw passthrough, and requiring one would defeat it: the
    // command exists to reach endpoints the typed commands cannot, including
    // workspace-less ones like /2.0/user. Wired at the bottom of the match
    // below -- where the Jira equivalent sits -- it would have failed with
    // "Workspace required" for anyone outside a Bitbucket checkout.
    if let BitbucketCommands::Api(api_args) = args.command {
        return crate::commands::api::run(&client, renderer, api_args).await;
    }

    // Detect git context for auto-detection
    let git_ctx = git::detect_git_context(bitbucket_remote);

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

            // Show git remotes if available (redact credentials)
            let remotes = git::get_all_remotes();
            if !remotes.is_empty() {
                error_msg.push_str("\nFound git remotes:\n");
                for (name, url) in &remotes {
                    let redacted = git::redact_url(url);
                    error_msg.push_str(&format!("  {} -> {}\n", name, redacted));
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

    // Kept apart for pipeline commands, which also take the repository as an
    // argument: an argument that disagrees with --repo is an error, while one
    // that differs from the git remote simply wins.
    let repo_flag = args.repo.clone();
    let git_repo = git_ctx.repo_slug.clone();
    let pipeline_repo = |positional: Option<String>, cmd: &str| -> anyhow::Result<String> {
        let chosen = positional::choose_repo(
            positional.as_deref(),
            repo_flag.as_deref(),
            git_repo.as_deref(),
        )?;
        require_repo(chosen.as_deref(), None, cmd)
    };

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
                default_reviewers,
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
                    default_reviewers,
                )
                .await
            }
            PrCommands::Update {
                repo,
                pr_id,
                title,
                description,
                reviewers,
            } => {
                pullrequests::update_pull_request(
                    &ctx,
                    &workspace,
                    &repo,
                    pr_id,
                    title.as_deref(),
                    description.as_deref(),
                    reviewers,
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
            PrCommands::Comment {
                repo,
                pr_id,
                text,
                parent,
                path,
                line,
                side,
            } => {
                pullrequests::add_pr_comment(
                    &ctx,
                    &workspace,
                    &repo,
                    pr_id,
                    &text,
                    parent,
                    path.as_deref(),
                    line,
                    side,
                )
                .await
            }
            PrCommands::ResolveComment {
                repo,
                pr_id,
                comment_id,
            } => pullrequests::resolve_pr_comment(&ctx, &workspace, &repo, pr_id, comment_id).await,
            PrCommands::ReopenComment {
                repo,
                pr_id,
                comment_id,
            } => pullrequests::reopen_pr_comment(&ctx, &workspace, &repo, pr_id, comment_id).await,
            PrCommands::Reviewers {
                repo,
                pr_id,
                add,
                all,
            } => {
                if add.is_empty() {
                    pullrequests::list_pr_reviewers(&ctx, &workspace, &repo, pr_id, all).await
                } else {
                    pullrequests::add_pr_reviewers(&ctx, &workspace, &repo, pr_id, add).await
                }
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
                repo_arg,
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
                let repo_slug = pipeline_repo(repo_arg, "pipeline list")?;

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
            PipelineCommands::Get {
                targets,
                pipeline_flag,
                steps,
            } => {
                let given = positional::split_leading_repo(
                    targets,
                    &["PIPELINE"],
                    &[pipeline_flag.is_some()],
                )?;
                let repo_slug = pipeline_repo(given.repo, "pipeline get")?;
                let id = resolve_pipeline_arg(given.ids.into_iter().next(), pipeline_flag)?;
                pipelines::get_pipeline(&ctx, &workspace, &repo_slug, &id, steps).await
            }
            PipelineCommands::Latest {
                repo_arg,
                branch,
                steps,
            } => {
                let repo_slug = pipeline_repo(repo_arg, "pipeline latest")?;

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
                repo_arg,
                ref_name,
                ref_type,
                variables,
                secured,
                custom_pipeline,
            } => {
                let repo_slug = pipeline_repo(repo_arg, "pipeline trigger")?;
                pipelines::trigger_pipeline(
                    &ctx,
                    &workspace,
                    &repo_slug,
                    &ref_name,
                    &ref_type,
                    variables,
                    secured,
                    custom_pipeline,
                )
                .await
            }
            PipelineCommands::Stop {
                targets,
                pipeline_flag,
            } => {
                let given = positional::split_leading_repo(
                    targets,
                    &["PIPELINE"],
                    &[pipeline_flag.is_some()],
                )?;
                let repo_slug = pipeline_repo(given.repo, "pipeline stop")?;
                let id = resolve_pipeline_arg(given.ids.into_iter().next(), pipeline_flag)?;
                let pipeline_uuid =
                    pipelines::resolve_pipeline_id(&ctx, &workspace, &repo_slug, &id).await?;
                pipelines::stop_pipeline(&ctx, &workspace, &repo_slug, &pipeline_uuid).await
            }
            PipelineCommands::Logs {
                targets,
                pipeline_flag,
                step_uuid_flag,
                step,
                grep,
                ignore_case,
                failed_only,
            } => {
                let given = positional::split_leading_repo(
                    targets,
                    &["PIPELINE", "STEP_UUID"],
                    &[pipeline_flag.is_some(), step_uuid_flag.is_some()],
                )?;
                let repo_slug = pipeline_repo(given.repo, "pipeline logs")?;
                // Positional identifiers fill the slots no flag supplied, in order.
                let mut ids = given.ids.into_iter();
                let pipeline_positional = if pipeline_flag.is_none() {
                    ids.next()
                } else {
                    None
                };
                let step_positional = if step_uuid_flag.is_none() {
                    ids.next()
                } else {
                    None
                };
                let id = resolve_pipeline_arg(pipeline_positional, pipeline_flag)?;
                let pipeline_uuid =
                    pipelines::resolve_pipeline_id(&ctx, &workspace, &repo_slug, &id).await?;
                let effective_step_uuid = step_positional.or(step_uuid_flag);
                pipelines::get_pipeline_logs(
                    &ctx,
                    &workspace,
                    &repo_slug,
                    &pipeline_uuid,
                    effective_step_uuid.as_deref(),
                    step.as_deref(),
                    grep.as_deref(),
                    ignore_case,
                    failed_only,
                )
                .await
            }
            PipelineCommands::Watch {
                targets,
                pipeline_flag,
                interval,
                steps,
                on_complete,
                timeout,
                log,
            } => {
                let given = positional::split_leading_repo(
                    targets,
                    &["PIPELINE"],
                    &[pipeline_flag.is_some()],
                )?;
                let repo_slug = pipeline_repo(given.repo, "pipeline watch")?;
                let id = resolve_pipeline_arg(given.ids.into_iter().next(), pipeline_flag)?;
                let final_status = pipelines::watch_pipeline(
                    &ctx,
                    &workspace,
                    &repo_slug,
                    &id,
                    interval,
                    steps,
                    on_complete.as_deref(),
                    timeout,
                    log,
                )
                .await?;
                let exit_code = pipelines::status_to_exit_code(&final_status);
                if exit_code != 0 {
                    std::process::exit(exit_code);
                }
                Ok(())
            }
            PipelineCommands::Steps {
                targets,
                pipeline_flag,
            } => {
                let given = positional::split_leading_repo(
                    targets,
                    &["PIPELINE"],
                    &[pipeline_flag.is_some()],
                )?;
                let repo_slug = pipeline_repo(given.repo, "pipeline steps")?;
                let id = resolve_pipeline_arg(given.ids.into_iter().next(), pipeline_flag)?;
                pipelines::list_steps(&ctx, &workspace, &repo_slug, &id).await
            }
            PipelineCommands::Status {
                repo_arg,
                steps,
                wait,
                interval,
            } => {
                let repo_slug = pipeline_repo(repo_arg, "pipeline status")?;
                pipelines::pipeline_status(&ctx, &workspace, &repo_slug, steps, wait, interval)
                    .await
            }
            PipelineCommands::Rerun {
                targets,
                pr,
                failed_only,
                variables,
                secured,
            } => {
                let given =
                    positional::split_leading_repo(targets, &["PIPELINE"], &[pr.is_some()])?;
                let repo_slug = pipeline_repo(given.repo, "pipeline rerun")?;
                let pipeline_id = given.ids.into_iter().next();

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
            BulkCommands::ArchiveRepos {
                days,
                execute,
                yes,
                dry_run,
            } => {
                // See the DeleteBranches arm: clap already rejects the pair, and
                // the `&&` keeps --dry-run a veto if that guard is ever removed.
                let execute = execute && !dry_run;
                bulk::disable_features_on_stale_repos(&ctx, &workspace, days, execute, yes).await
            }
            BulkCommands::DeleteBranches {
                repo,
                exclude,
                execute,
                yes,
                dry_run,
            } => {
                // `--dry-run` used to be the only thing standing between a bare
                // invocation and mass deletion. Listing is now the default, so
                // the flag is redundant. clap rejects it alongside --execute,
                // making this `&&` redundant too; it stays as the belt to that
                // braces, so removing the `conflicts_with` can never silently
                // turn `--dry-run` into a no-op that deletes.
                let execute = execute && !dry_run;
                bulk::delete_branches(&ctx, &workspace, &repo, exclude, execute, yes).await
            }
        },
        BitbucketCommands::Whoami | BitbucketCommands::Api(_) => {
            unreachable!("handled above, before the workspace gate")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_pipeline_arg_positional() {
        let result = resolve_pipeline_arg(Some("13".to_string()), None);
        assert_eq!(result.unwrap(), "13");
    }

    #[test]
    fn test_resolve_pipeline_arg_flag() {
        let result = resolve_pipeline_arg(None, Some("42".to_string()));
        assert_eq!(result.unwrap(), "42");
    }

    #[test]
    fn test_resolve_pipeline_arg_positional_takes_precedence() {
        let result = resolve_pipeline_arg(Some("13".to_string()), Some("42".to_string()));
        assert_eq!(result.unwrap(), "13");
    }

    #[test]
    fn test_resolve_pipeline_arg_neither() {
        let result = resolve_pipeline_arg(None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Pipeline ID required"));
    }
}
