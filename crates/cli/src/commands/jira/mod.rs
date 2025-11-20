use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};

// Submodules
mod audit;
mod automation;
mod bulk;
mod fields_workflows;
mod issues;
mod projects;
pub mod utils;
mod webhooks;

use utils::JiraContext;

#[derive(Args, Debug, Clone)]
pub struct JiraArgs {
    #[command(subcommand)]
    command: JiraCommands,
}

#[derive(Subcommand, Debug, Clone)]
enum JiraCommands {
    /// Search issues using JQL or filter parameters
    Search {
        /// Raw JQL query (conflicts with filter flags)
        #[arg(long, conflicts_with_all = ["assignee", "status", "priority", "label", "type", "project", "text"])]
        jql: Option<String>,

        // Filter flags (only when --jql not used)
        /// Filter by assignee (use @me for current user)
        #[arg(short = 'a', long)]
        assignee: Option<String>,

        /// Filter by status (repeatable)
        #[arg(short = 's', long, num_args = 0..)]
        status: Vec<String>,

        /// Filter by priority
        #[arg(short = 'y', long)]
        priority: Option<String>,

        /// Filter by label (repeatable)
        #[arg(short = 'l', long, num_args = 0..)]
        label: Vec<String>,

        /// Filter by issue type
        #[arg(short = 't', long)]
        r#type: Option<String>,

        /// Filter by project
        #[arg(short = 'p', long)]
        project: Option<String>,

        /// Free text search in summary
        #[arg(long)]
        text: Option<String>,

        /// Display generated JQL query
        #[arg(long)]
        show_query: bool,

        /// Maximum number of issues to return
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },

    /// Fetch a single issue
    Get {
        /// Issue key (e.g. DEV-123)
        key: String,
    },

    /// Create a new issue
    Create {
        /// Project key
        #[arg(long)]
        project: String,
        /// Issue type (e.g. Task, Bug, Story)
        #[arg(long)]
        issue_type: String,
        /// Issue summary
        #[arg(long)]
        summary: String,
        /// Issue description
        #[arg(long)]
        description: Option<String>,
        /// Assignee account ID or email
        #[arg(long)]
        assignee: Option<String>,
        /// Priority name (e.g. High, Medium, Low)
        #[arg(long)]
        priority: Option<String>,
    },

    /// Update an existing issue
    Update {
        /// Issue key
        key: String,
        /// New summary
        #[arg(long)]
        summary: Option<String>,
        /// New description
        #[arg(long)]
        description: Option<String>,
        /// New priority
        #[arg(long)]
        priority: Option<String>,
    },

    /// Delete an issue
    Delete {
        /// Issue key
        key: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Transition an issue to a new status
    Transition {
        /// Issue key
        key: String,
        /// Transition name or ID
        #[arg(long)]
        transition: String,
    },

    /// Assign issue to user
    Assign {
        /// Issue key
        key: String,
        /// User account ID or email
        #[arg(long)]
        assignee: String,
    },

    /// Unassign issue
    Unassign {
        /// Issue key
        key: String,
    },

    /// Manage issue watchers
    #[command(subcommand)]
    Watchers(WatcherCommands),

    /// Manage issue links
    #[command(subcommand)]
    Links(LinkCommands),

    /// Manage issue comments
    #[command(subcommand)]
    Comments(CommentCommands),

    /// Manage projects
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Manage components
    #[command(subcommand)]
    Components(ComponentCommands),

    /// Manage versions/releases
    #[command(subcommand)]
    Versions(VersionCommands),

    /// Manage project roles
    #[command(subcommand)]
    Roles(RoleCommands),

    /// Manage custom fields
    #[command(subcommand)]
    Fields(FieldCommands),

    /// Manage workflows
    #[command(subcommand)]
    Workflows(WorkflowCommands),

    /// Bulk operations
    #[command(subcommand)]
    Bulk(BulkCommands),

    /// Manage automation rules
    #[command(subcommand)]
    Automation(AutomationCommands),

    /// Manage webhooks
    #[command(subcommand)]
    Webhooks(WebhookCommands),

    /// Audit log access
    #[command(subcommand)]
    Audit(AuditCommands),
}

#[derive(Subcommand, Debug, Clone)]
enum WatcherCommands {
    /// List watchers for an issue
    List { key: String },
    /// Add watcher to an issue
    Add {
        key: String,
        /// User account ID or email
        user: String,
    },
    /// Remove watcher from an issue
    Remove {
        key: String,
        /// User account ID or email
        user: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum LinkCommands {
    /// List links for an issue
    List { key: String },
    /// Create a link between two issues
    Create {
        /// Source issue key
        from: String,
        /// Target issue key
        to: String,
        /// Link type (e.g. blocks, relates-to)
        #[arg(long)]
        link_type: String,
    },
    /// Delete an issue link
    Delete {
        /// Link ID
        link_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum CommentCommands {
    /// List comments on an issue
    List { key: String },
    /// Add a comment to an issue
    Add {
        key: String,
        /// Comment body
        #[arg(long)]
        body: String,
    },
    /// Update a comment
    Update {
        /// Comment ID
        comment_id: String,
        /// New comment body
        #[arg(long)]
        body: String,
    },
    /// Delete a comment
    Delete {
        /// Comment ID
        comment_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum ProjectCommands {
    /// List all projects
    List,
    /// Get project details
    Get {
        /// Project key
        key: String,
    },
    /// Create a new project
    Create {
        /// Project key (e.g. PROJ)
        #[arg(long)]
        key: String,
        /// Project name
        #[arg(long)]
        name: String,
        /// Project type: software, service_desk, business
        #[arg(long, default_value = "software")]
        project_type: String,
        /// Lead account ID
        #[arg(long)]
        lead: Option<String>,
        /// Description
        #[arg(long)]
        description: Option<String>,
    },
    /// Update project
    Update {
        /// Project key
        key: String,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New description
        #[arg(long)]
        description: Option<String>,
        /// New lead account ID
        #[arg(long)]
        lead: Option<String>,
    },
    /// Delete project
    Delete {
        /// Project key
        key: String,
        /// Skip confirmation
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum ComponentCommands {
    /// List components in a project
    List {
        /// Project key
        project: String,
    },
    /// Get component details
    Get {
        /// Component ID
        id: String,
    },
    /// Create a component
    Create {
        /// Project key
        #[arg(long)]
        project: String,
        /// Component name
        #[arg(long)]
        name: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
        /// Lead account ID
        #[arg(long)]
        lead: Option<String>,
    },
    /// Update a component
    Update {
        /// Component ID
        id: String,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New description
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete a component
    Delete {
        /// Component ID
        id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum VersionCommands {
    /// List versions in a project
    List {
        /// Project key
        project: String,
    },
    /// Get version details
    Get {
        /// Version ID
        id: String,
    },
    /// Create a version
    Create {
        /// Project key
        #[arg(long)]
        project: String,
        /// Version name
        #[arg(long)]
        name: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start_date: Option<String>,
        /// Release date (YYYY-MM-DD)
        #[arg(long)]
        release_date: Option<String>,
        /// Mark as released
        #[arg(long)]
        released: bool,
        /// Mark as archived
        #[arg(long)]
        archived: bool,
    },
    /// Update a version
    Update {
        /// Version ID
        id: String,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New description
        #[arg(long)]
        description: Option<String>,
        /// Released status
        #[arg(long)]
        released: Option<bool>,
        /// Archived status
        #[arg(long)]
        archived: Option<bool>,
    },
    /// Delete a version
    Delete {
        /// Version ID
        id: String,
    },
    /// Merge versions
    Merge {
        /// Source version ID
        from: String,
        /// Target version ID
        to: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum RoleCommands {
    /// List project roles
    List {
        /// Project key
        project: String,
    },
    /// Get role details
    Get {
        /// Project key
        project: String,
        /// Role ID
        role_id: String,
    },
    /// List actors for a role
    Actors {
        /// Project key
        project: String,
        /// Role ID
        role_id: String,
    },
    /// Add actor to role
    AddActor {
        /// Project key
        project: String,
        /// Role ID
        role_id: String,
        /// User account ID
        #[arg(long)]
        user: String,
    },
    /// Remove actor from role
    RemoveActor {
        /// Project key
        project: String,
        /// Role ID
        role_id: String,
        /// User account ID
        #[arg(long)]
        user: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum FieldCommands {
    /// List all fields
    List,
    /// Get field details
    Get {
        /// Field ID
        id: String,
    },
    /// Create custom field
    Create {
        /// Field name
        #[arg(long)]
        name: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
        /// Field type
        #[arg(long)]
        field_type: String,
    },
    /// Delete custom field
    Delete {
        /// Field ID
        id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum WorkflowCommands {
    /// List all workflows
    List,
    /// Get workflow details
    Get {
        /// Workflow name
        name: String,
    },
    /// Export workflow to JSON
    Export {
        /// Workflow name
        name: String,
        /// Output file path
        #[arg(long)]
        output: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum BulkCommands {
    /// Bulk transition issues
    Transition {
        /// JQL query to select issues
        #[arg(long)]
        jql: String,
        /// Transition name or ID
        #[arg(long)]
        transition: String,
        /// Dry run mode
        #[arg(long)]
        dry_run: bool,
        /// Concurrency level
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
    /// Bulk assign issues
    Assign {
        /// JQL query to select issues
        #[arg(long)]
        jql: String,
        /// Assignee account ID
        #[arg(long)]
        assignee: String,
        /// Dry run mode
        #[arg(long)]
        dry_run: bool,
        /// Concurrency level
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
    /// Bulk label operations
    Label {
        /// JQL query to select issues
        #[arg(long)]
        jql: String,
        /// Action: add, remove, or set
        #[arg(long)]
        action: String,
        /// Labels to apply
        #[arg(long, value_delimiter = ',')]
        labels: Vec<String>,
        /// Dry run mode
        #[arg(long)]
        dry_run: bool,
        /// Concurrency level
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
    /// Export issues to file
    Export {
        /// JQL query to select issues
        #[arg(long)]
        jql: String,
        /// Output file path
        #[arg(long)]
        output: std::path::PathBuf,
        /// Export format: json or csv
        #[arg(long, default_value = "json")]
        format: String,
        /// Fields to include (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Vec<String>,
    },
    /// Import issues from file
    Import {
        /// Input file path (JSON)
        #[arg(long)]
        file: std::path::PathBuf,
        /// Target project key
        #[arg(long)]
        project: String,
        /// Dry run mode
        #[arg(long)]
        dry_run: bool,
        /// Concurrency level
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum AutomationCommands {
    /// List all automation rules
    List,
    /// Get automation rule details
    Get {
        /// Rule ID
        rule_id: i64,
    },
    /// Create a new automation rule
    Create {
        /// Rule name
        #[arg(long)]
        name: String,
        /// Rule description
        #[arg(long)]
        description: Option<String>,
        /// Path to rule definition JSON file
        #[arg(long)]
        definition: std::path::PathBuf,
    },
    /// Update an automation rule
    Update {
        /// Rule ID
        rule_id: i64,
        /// New rule name
        #[arg(long)]
        name: Option<String>,
        /// New rule description
        #[arg(long)]
        description: Option<String>,
    },
    /// Enable an automation rule
    Enable {
        /// Rule ID
        rule_id: i64,
    },
    /// Disable an automation rule
    Disable {
        /// Rule ID
        rule_id: i64,
    },
    /// Delete an automation rule
    Delete {
        /// Rule ID
        rule_id: i64,
        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },
    /// Export an automation rule
    Export {
        /// Rule ID
        rule_id: i64,
        /// Output file path
        #[arg(long)]
        output: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum WebhookCommands {
    /// List all webhooks
    List,
    /// Get webhook details
    Get {
        /// Webhook ID
        webhook_id: i64,
    },
    /// Create a new webhook
    Create {
        /// Webhook name
        #[arg(long)]
        name: String,
        /// Webhook URL
        #[arg(long)]
        url: String,
        /// Events to listen for (comma-separated)
        #[arg(long, value_delimiter = ',')]
        events: Vec<String>,
        /// Enable webhook immediately
        #[arg(long)]
        enabled: bool,
        /// JQL filter for events
        #[arg(long)]
        jql_filter: Option<String>,
    },
    /// Update a webhook
    Update {
        /// Webhook ID
        webhook_id: i64,
        /// New webhook name
        #[arg(long)]
        name: Option<String>,
        /// New webhook URL
        #[arg(long)]
        url: Option<String>,
        /// New events list (comma-separated)
        #[arg(long, value_delimiter = ',')]
        events: Option<Vec<String>>,
        /// Enable or disable
        #[arg(long)]
        enabled: Option<bool>,
    },
    /// Enable a webhook
    Enable {
        /// Webhook ID
        webhook_id: i64,
    },
    /// Disable a webhook
    Disable {
        /// Webhook ID
        webhook_id: i64,
    },
    /// Delete a webhook
    Delete {
        /// Webhook ID
        webhook_id: i64,
        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },
    /// Test a webhook
    Test {
        /// Webhook ID
        webhook_id: i64,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum AuditCommands {
    /// List audit records
    List {
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD)
        #[arg(long)]
        to: Option<String>,
        /// Filter by event type
        #[arg(long)]
        filter: Option<String>,
        /// Maximum number of records
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Export audit records
    Export {
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD)
        #[arg(long)]
        to: Option<String>,
        /// Filter by event type
        #[arg(long)]
        filter: Option<String>,
        /// Output file path
        #[arg(long)]
        output: std::path::PathBuf,
        /// Export format: json or csv
        #[arg(long, default_value = "json")]
        format: String,
    },
}

pub async fn execute(args: JiraArgs, client: ApiClient, renderer: &OutputRenderer) -> Result<()> {
    let ctx = JiraContext { client, renderer };

    match args.command {
        JiraCommands::Search {
            jql,
            assignee,
            status,
            priority,
            label,
            r#type,
            project,
            text,
            show_query,
            limit,
        } => {
            issues::search_issues(
                &ctx,
                jql.as_deref(),
                assignee.as_deref(),
                &status,
                priority.as_deref(),
                &label,
                r#type.as_deref(),
                project.as_deref(),
                text.as_deref(),
                show_query,
                limit,
            )
            .await
        }
        JiraCommands::Get { key } => issues::view_issue(&ctx, &key).await,
        JiraCommands::Create {
            project,
            issue_type,
            summary,
            description,
            assignee,
            priority,
        } => {
            issues::create_issue(
                &ctx,
                &project,
                &issue_type,
                &summary,
                description.as_deref(),
                assignee.as_deref(),
                priority.as_deref(),
            )
            .await
        }
        JiraCommands::Update {
            key,
            summary,
            description,
            priority,
        } => {
            issues::update_issue(
                &ctx,
                &key,
                summary.as_deref(),
                description.as_deref(),
                priority.as_deref(),
            )
            .await
        }
        JiraCommands::Delete { key, force } => issues::delete_issue(&ctx, &key, force).await,
        JiraCommands::Transition { key, transition } => {
            issues::transition_issue(&ctx, &key, &transition).await
        }
        JiraCommands::Assign { key, assignee } => issues::assign_issue(&ctx, &key, &assignee).await,
        JiraCommands::Unassign { key } => issues::unassign_issue(&ctx, &key).await,
        JiraCommands::Watchers(cmd) => match cmd {
            WatcherCommands::List { key } => issues::list_watchers(&ctx, &key).await,
            WatcherCommands::Add { key, user } => issues::add_watcher(&ctx, &key, &user).await,
            WatcherCommands::Remove { key, user } => {
                issues::remove_watcher(&ctx, &key, &user).await
            }
        },
        JiraCommands::Links(cmd) => match cmd {
            LinkCommands::List { key } => issues::list_links(&ctx, &key).await,
            LinkCommands::Create {
                from,
                to,
                link_type,
            } => issues::create_link(&ctx, &from, &to, &link_type).await,
            LinkCommands::Delete { link_id } => issues::delete_link(&ctx, &link_id).await,
        },
        JiraCommands::Comments(cmd) => match cmd {
            CommentCommands::List { key } => issues::list_comments(&ctx, &key).await,
            CommentCommands::Add { key, body } => issues::add_comment(&ctx, &key, &body).await,
            CommentCommands::Update { comment_id, body } => {
                issues::update_comment(&ctx, &comment_id, &body).await
            }
            CommentCommands::Delete { comment_id } => {
                issues::delete_comment(&ctx, &comment_id).await
            }
        },
        JiraCommands::Project(cmd) => match cmd {
            ProjectCommands::List => projects::list_projects(&ctx).await,
            ProjectCommands::Get { key } => projects::get_project(&ctx, &key).await,
            ProjectCommands::Create {
                key,
                name,
                project_type,
                lead,
                description,
            } => {
                projects::create_project(
                    &ctx,
                    &key,
                    &name,
                    &project_type,
                    lead.as_deref(),
                    description.as_deref(),
                )
                .await
            }
            ProjectCommands::Update {
                key,
                name,
                description,
                lead,
            } => {
                projects::update_project(
                    &ctx,
                    &key,
                    name.as_deref(),
                    description.as_deref(),
                    lead.as_deref(),
                )
                .await
            }
            ProjectCommands::Delete { key, force } => {
                projects::delete_project(&ctx, &key, force).await
            }
        },
        JiraCommands::Components(cmd) => match cmd {
            ComponentCommands::List { project } => projects::list_components(&ctx, &project).await,
            ComponentCommands::Get { id } => projects::get_component(&ctx, &id).await,
            ComponentCommands::Create {
                project,
                name,
                description,
                lead,
            } => {
                projects::create_component(
                    &ctx,
                    &project,
                    &name,
                    description.as_deref(),
                    lead.as_deref(),
                )
                .await
            }
            ComponentCommands::Update {
                id,
                name,
                description,
            } => {
                projects::update_component(&ctx, &id, name.as_deref(), description.as_deref()).await
            }
            ComponentCommands::Delete { id } => projects::delete_component(&ctx, &id).await,
        },
        JiraCommands::Versions(cmd) => match cmd {
            VersionCommands::List { project } => projects::list_versions(&ctx, &project).await,
            VersionCommands::Get { id } => projects::get_version(&ctx, &id).await,
            VersionCommands::Create {
                project,
                name,
                description,
                start_date,
                release_date,
                released,
                archived,
            } => {
                projects::create_version(
                    &ctx,
                    &project,
                    &name,
                    description.as_deref(),
                    start_date.as_deref(),
                    release_date.as_deref(),
                    released,
                    archived,
                )
                .await
            }
            VersionCommands::Update {
                id,
                name,
                description,
                released,
                archived,
            } => {
                projects::update_version(
                    &ctx,
                    &id,
                    name.as_deref(),
                    description.as_deref(),
                    released,
                    archived,
                )
                .await
            }
            VersionCommands::Delete { id } => projects::delete_version(&ctx, &id).await,
            VersionCommands::Merge { from, to } => projects::merge_versions(&ctx, &from, &to).await,
        },
        JiraCommands::Roles(cmd) => match cmd {
            RoleCommands::List { project } => fields_workflows::list_roles(&ctx, &project).await,
            RoleCommands::Get { project, role_id } => {
                fields_workflows::get_role(&ctx, &project, &role_id).await
            }
            RoleCommands::Actors { project, role_id } => {
                fields_workflows::list_role_actors(&ctx, &project, &role_id).await
            }
            RoleCommands::AddActor {
                project,
                role_id,
                user,
            } => fields_workflows::add_role_actor(&ctx, &project, &role_id, &user).await,
            RoleCommands::RemoveActor {
                project,
                role_id,
                user,
            } => fields_workflows::remove_role_actor(&ctx, &project, &role_id, &user).await,
        },
        JiraCommands::Fields(cmd) => match cmd {
            FieldCommands::List => fields_workflows::list_fields(&ctx).await,
            FieldCommands::Get { id } => fields_workflows::get_field(&ctx, &id).await,
            FieldCommands::Create {
                name,
                description,
                field_type,
            } => {
                fields_workflows::create_field(&ctx, &name, description.as_deref(), &field_type)
                    .await
            }
            FieldCommands::Delete { id } => fields_workflows::delete_field(&ctx, &id).await,
        },
        JiraCommands::Workflows(cmd) => match cmd {
            WorkflowCommands::List => fields_workflows::list_workflows(&ctx).await,
            WorkflowCommands::Get { name } => fields_workflows::get_workflow(&ctx, &name).await,
            WorkflowCommands::Export { name, output } => {
                fields_workflows::export_workflow(&ctx, &name, output.as_deref()).await
            }
        },
        JiraCommands::Bulk(cmd) => match cmd {
            BulkCommands::Transition {
                jql,
                transition,
                dry_run,
                concurrency,
            } => bulk::bulk_transition(&ctx, &jql, &transition, dry_run, concurrency).await,
            BulkCommands::Assign {
                jql,
                assignee,
                dry_run,
                concurrency,
            } => bulk::bulk_assign(&ctx, &jql, &assignee, dry_run, concurrency).await,
            BulkCommands::Label {
                jql,
                action,
                labels,
                dry_run,
                concurrency,
            } => {
                let label_action = match action.to_lowercase().as_str() {
                    "add" => bulk::LabelAction::Add,
                    "remove" => bulk::LabelAction::Remove,
                    "set" => bulk::LabelAction::Set,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid action '{}'. Must be one of: add, remove, set",
                            action
                        ))
                    }
                };
                bulk::bulk_label(&ctx, &jql, label_action, labels, dry_run, concurrency).await
            }
            BulkCommands::Export {
                jql,
                output,
                format,
                fields,
            } => {
                let export_format = match format.to_lowercase().as_str() {
                    "json" => bulk::ExportFormat::Json,
                    "csv" => bulk::ExportFormat::Csv,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid format '{}'. Must be one of: json, csv",
                            format
                        ))
                    }
                };
                bulk::bulk_export(&ctx, &jql, &output, export_format, fields).await
            }
            BulkCommands::Import {
                file,
                project,
                dry_run,
                concurrency,
            } => bulk::bulk_import(&ctx, &file, &project, dry_run, concurrency).await,
        },
        JiraCommands::Automation(cmd) => match cmd {
            AutomationCommands::List => automation::list_rules(&ctx).await,
            AutomationCommands::Get { rule_id } => automation::get_rule(&ctx, rule_id).await,
            AutomationCommands::Create {
                name,
                description,
                definition,
            } => automation::create_rule(&ctx, &name, description.as_deref(), &definition).await,
            AutomationCommands::Update {
                rule_id,
                name,
                description,
            } => {
                automation::update_rule(&ctx, rule_id, name.as_deref(), description.as_deref())
                    .await
            }
            AutomationCommands::Enable { rule_id } => automation::enable_rule(&ctx, rule_id).await,
            AutomationCommands::Disable { rule_id } => {
                automation::disable_rule(&ctx, rule_id).await
            }
            AutomationCommands::Delete { rule_id, force } => {
                automation::delete_rule(&ctx, rule_id, force).await
            }
            AutomationCommands::Export { rule_id, output } => {
                automation::export_rule(&ctx, rule_id, output.as_ref()).await
            }
        },
        JiraCommands::Webhooks(cmd) => match cmd {
            WebhookCommands::List => webhooks::list_webhooks(&ctx).await,
            WebhookCommands::Get { webhook_id } => webhooks::get_webhook(&ctx, webhook_id).await,
            WebhookCommands::Create {
                name,
                url,
                events,
                enabled,
                jql_filter,
            } => {
                webhooks::create_webhook(&ctx, &name, &url, events, enabled, jql_filter.as_deref())
                    .await
            }
            WebhookCommands::Update {
                webhook_id,
                name,
                url,
                events,
                enabled,
            } => {
                webhooks::update_webhook(
                    &ctx,
                    webhook_id,
                    name.as_deref(),
                    url.as_deref(),
                    events,
                    enabled,
                )
                .await
            }
            WebhookCommands::Enable { webhook_id } => {
                webhooks::enable_webhook(&ctx, webhook_id).await
            }
            WebhookCommands::Disable { webhook_id } => {
                webhooks::disable_webhook(&ctx, webhook_id).await
            }
            WebhookCommands::Delete { webhook_id, force } => {
                webhooks::delete_webhook(&ctx, webhook_id, force).await
            }
            WebhookCommands::Test { webhook_id } => webhooks::test_webhook(&ctx, webhook_id).await,
        },
        JiraCommands::Audit(cmd) => match cmd {
            AuditCommands::List {
                from,
                to,
                filter,
                limit,
            } => {
                audit::list_audit_records(
                    &ctx,
                    from.as_deref(),
                    to.as_deref(),
                    filter.as_deref(),
                    limit,
                )
                .await
            }
            AuditCommands::Export {
                from,
                to,
                filter,
                output,
                format,
            } => {
                let export_format = match format.to_lowercase().as_str() {
                    "json" => audit::ExportFormat::Json,
                    "csv" => audit::ExportFormat::Csv,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid format '{}'. Must be one of: json, csv",
                            format
                        ))
                    }
                };
                audit::export_audit_records(
                    &ctx,
                    from.as_deref(),
                    to.as_deref(),
                    filter.as_deref(),
                    &output,
                    export_format,
                )
                .await
            }
        },
    }
}
