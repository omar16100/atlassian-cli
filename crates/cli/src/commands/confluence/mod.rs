use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};

// Submodules
mod analytics;
mod attachments;
mod bulk;
mod pages;
mod search;
mod spaces;
pub mod utils;

use utils::ConfluenceContext;

#[derive(Args, Debug, Clone)]
pub struct ConfluenceArgs {
    #[command(subcommand)]
    command: ConfluenceCommands,
}

#[derive(Subcommand, Debug, Clone)]
enum ConfluenceCommands {
    /// Space operations
    #[command(subcommand)]
    Space(SpaceCommands),

    /// Page operations
    #[command(subcommand)]
    Page(PageCommands),

    /// Blog post operations
    #[command(subcommand)]
    Blog(BlogCommands),

    /// Attachment operations
    #[command(subcommand)]
    Attachment(AttachmentCommands),

    /// Search operations
    #[command(subcommand)]
    Search(SearchCommands),

    /// Bulk operations
    #[command(subcommand)]
    Bulk(BulkCommands),

    /// Analytics operations
    #[command(subcommand)]
    Analytics(AnalyticsCommands),
}

#[derive(Subcommand, Debug, Clone)]
enum SpaceCommands {
    /// List spaces
    List {
        /// Maximum number of results
        #[arg(long)]
        limit: Option<usize>,
        /// Filter by space type (global, personal)
        #[arg(long)]
        space_type: Option<String>,
    },
    /// Get space details
    Get {
        /// Space key
        key: String,
    },
    /// Create a new space
    Create {
        /// Space key
        #[arg(long)]
        key: String,
        /// Space name
        #[arg(long)]
        name: String,
        /// Space description
        #[arg(long)]
        description: Option<String>,
    },
    /// Update space
    Update {
        /// Space ID
        space_id: String,
        /// New space name
        #[arg(long)]
        name: Option<String>,
        /// New space description
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete space
    Delete {
        /// Space ID
        space_id: String,
        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },
    /// Get space permissions
    Permissions {
        /// Space key
        key: String,
    },
    /// Add space permission
    AddPermission {
        /// Space key
        key: String,
        /// Permission type (read, write, admin)
        #[arg(long)]
        permission: String,
        /// Subject type (user, group)
        #[arg(long)]
        subject_type: String,
        /// Subject identifier (user ID or group name)
        #[arg(long)]
        subject_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum PageCommands {
    /// List pages
    List {
        /// Filter by space key
        #[arg(long)]
        space: Option<String>,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Get page details
    Get {
        /// Page ID
        page_id: String,
    },
    /// Create a new page
    Create {
        /// Space ID
        #[arg(long)]
        space: String,
        /// Page title
        #[arg(long)]
        title: String,
        /// Body content file (HTML storage format)
        #[arg(long)]
        body: Option<std::path::PathBuf>,
        /// Parent page ID
        #[arg(long)]
        parent: Option<String>,
    },
    /// Update a page
    Update {
        /// Page ID
        page_id: String,
        /// New page title
        #[arg(long)]
        title: Option<String>,
        /// New body content file (HTML storage format)
        #[arg(long)]
        body: Option<std::path::PathBuf>,
    },
    /// Delete a page
    Delete {
        /// Page ID
        page_id: String,
        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },
    /// List page versions
    Versions {
        /// Page ID
        page_id: String,
    },
    /// Add label to page
    AddLabel {
        /// Page ID
        page_id: String,
        /// Label name
        label: String,
    },
    /// Remove label from page
    RemoveLabel {
        /// Page ID
        page_id: String,
        /// Label name
        label: String,
    },
    /// List page comments
    Comments {
        /// Page ID
        page_id: String,
    },
    /// Add comment to page
    AddComment {
        /// Page ID
        page_id: String,
        /// Comment text
        comment: String,
    },
    /// Get page restrictions
    GetRestrictions {
        /// Page ID
        page_id: String,
    },
    /// Add page restriction
    AddRestriction {
        /// Page ID
        page_id: String,
        /// Operation (read, update)
        #[arg(long)]
        operation: String,
        /// Subject type (user, group)
        #[arg(long)]
        subject_type: String,
        /// Subject identifier (user ID or group name)
        #[arg(long)]
        subject_id: String,
    },
    /// Remove page restriction
    RemoveRestriction {
        /// Page ID
        page_id: String,
        /// Operation (read, update)
        #[arg(long)]
        operation: String,
        /// Subject type (user, group)
        #[arg(long)]
        subject_type: String,
        /// Subject identifier (user ID or group name)
        #[arg(long)]
        subject_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum BlogCommands {
    /// List blog posts
    List {
        /// Filter by space ID
        #[arg(long)]
        space: Option<String>,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Get blog post details
    Get {
        /// Blog post ID
        blogpost_id: String,
    },
    /// Create a blog post
    Create {
        /// Space ID
        #[arg(long)]
        space: String,
        /// Blog post title
        #[arg(long)]
        title: String,
        /// Body content file (HTML storage format)
        #[arg(long)]
        body: Option<std::path::PathBuf>,
    },
    /// Update a blog post
    Update {
        /// Blog post ID
        blogpost_id: String,
        /// New blog post title
        #[arg(long)]
        title: Option<String>,
        /// New body content file (HTML storage format)
        #[arg(long)]
        body: Option<std::path::PathBuf>,
    },
    /// Delete a blog post
    Delete {
        /// Blog post ID
        blogpost_id: String,
        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum AttachmentCommands {
    /// List attachments for a page
    List {
        /// Page ID
        page_id: String,
    },
    /// Get attachment details
    Get {
        /// Attachment ID
        attachment_id: String,
    },
    /// Upload an attachment
    Upload {
        /// Page ID
        page_id: String,
        /// File path to upload
        #[arg(long)]
        file: std::path::PathBuf,
        /// Optional comment
        #[arg(long)]
        comment: Option<String>,
    },
    /// Download an attachment
    Download {
        /// Attachment ID
        attachment_id: String,
        /// Output file path
        #[arg(long)]
        output: std::path::PathBuf,
    },
    /// Delete an attachment
    Delete {
        /// Attachment ID
        attachment_id: String,
        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum SearchCommands {
    /// Search using CQL
    Cql {
        /// CQL query
        query: String,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Text search
    Text {
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Search in space
    InSpace {
        /// Space key
        space: String,
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Search using filter parameters
    Params {
        /// Filter by space key
        #[arg(short = 's', long)]
        space: Option<String>,

        /// Filter by content type (page, blogpost, attachment)
        #[arg(short = 't', long)]
        r#type: Option<String>,

        /// Filter by creator (use @me for current user)
        #[arg(short = 'c', long)]
        creator: Option<String>,

        /// Filter by label (repeatable)
        #[arg(short = 'l', long, num_args = 0..)]
        label: Vec<String>,

        /// Search in title
        #[arg(long)]
        title: Option<String>,

        /// Free text search
        #[arg(long)]
        text: Option<String>,

        /// Display generated CQL query
        #[arg(long)]
        show_query: bool,

        /// Maximum number of results
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum BulkCommands {
    /// Bulk delete pages
    Delete {
        /// CQL query to select pages
        #[arg(long)]
        cql: String,
        /// Dry run mode
        #[arg(long)]
        dry_run: bool,
        /// Concurrency level
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
    /// Bulk add labels
    AddLabels {
        /// CQL query to select pages
        #[arg(long)]
        cql: String,
        /// Labels to add (comma-separated)
        #[arg(long, value_delimiter = ',')]
        labels: Vec<String>,
        /// Dry run mode
        #[arg(long)]
        dry_run: bool,
        /// Concurrency level
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
    /// Bulk export pages
    Export {
        /// CQL query to select pages
        #[arg(long)]
        cql: String,
        /// Output file path
        #[arg(long)]
        output: std::path::PathBuf,
        /// Export format: json or csv
        #[arg(long, default_value = "json")]
        format: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum AnalyticsCommands {
    /// Get page view statistics
    PageViews {
        /// Page ID
        page_id: String,
        /// From date (YYYY-MM-DD)
        #[arg(long)]
        from: Option<String>,
    },
    /// Get space analytics
    SpaceStats {
        /// Space key
        space_key: String,
    },
}

pub async fn execute(
    args: ConfluenceArgs,
    client: ApiClient,
    renderer: &OutputRenderer,
) -> Result<()> {
    let ctx = ConfluenceContext { client, renderer };

    match args.command {
        ConfluenceCommands::Space(cmd) => match cmd {
            SpaceCommands::List { limit, space_type } => {
                spaces::list_spaces(&ctx, limit, space_type.as_deref()).await
            }
            SpaceCommands::Get { key } => spaces::get_space(&ctx, &key).await,
            SpaceCommands::Create {
                key,
                name,
                description,
            } => spaces::create_space(&ctx, &key, &name, description.as_deref()).await,
            SpaceCommands::Update {
                space_id,
                name,
                description,
            } => {
                spaces::update_space(&ctx, &space_id, name.as_deref(), description.as_deref()).await
            }
            SpaceCommands::Delete { space_id, force } => {
                spaces::delete_space(&ctx, &space_id, force).await
            }
            SpaceCommands::Permissions { key } => spaces::get_space_permissions(&ctx, &key).await,
            SpaceCommands::AddPermission {
                key,
                permission,
                subject_type,
                subject_id,
            } => {
                spaces::add_space_permission(&ctx, &key, &permission, &subject_type, &subject_id)
                    .await
            }
        },
        ConfluenceCommands::Page(cmd) => match cmd {
            PageCommands::List { space, limit } => {
                pages::list_pages(&ctx, space.as_deref(), limit).await
            }
            PageCommands::Get { page_id } => pages::get_page(&ctx, &page_id).await,
            PageCommands::Create {
                space,
                title,
                body,
                parent,
            } => pages::create_page(&ctx, &space, &title, body.as_ref(), parent.as_deref()).await,
            PageCommands::Update {
                page_id,
                title,
                body,
            } => pages::update_page(&ctx, &page_id, title.as_deref(), body.as_ref()).await,
            PageCommands::Delete { page_id, force } => {
                pages::delete_page(&ctx, &page_id, force).await
            }
            PageCommands::Versions { page_id } => pages::list_page_versions(&ctx, &page_id).await,
            PageCommands::AddLabel { page_id, label } => {
                pages::add_page_label(&ctx, &page_id, &label).await
            }
            PageCommands::RemoveLabel { page_id, label } => {
                pages::remove_page_label(&ctx, &page_id, &label).await
            }
            PageCommands::Comments { page_id } => pages::list_page_comments(&ctx, &page_id).await,
            PageCommands::AddComment { page_id, comment } => {
                pages::add_page_comment(&ctx, &page_id, &comment).await
            }
            PageCommands::GetRestrictions { page_id } => {
                pages::get_page_restrictions(&ctx, &page_id).await
            }
            PageCommands::AddRestriction {
                page_id,
                operation,
                subject_type,
                subject_id,
            } => {
                pages::add_page_restriction(&ctx, &page_id, &operation, &subject_type, &subject_id)
                    .await
            }
            PageCommands::RemoveRestriction {
                page_id,
                operation,
                subject_type,
                subject_id,
            } => {
                pages::remove_page_restriction(
                    &ctx,
                    &page_id,
                    &operation,
                    &subject_type,
                    &subject_id,
                )
                .await
            }
        },
        ConfluenceCommands::Blog(cmd) => match cmd {
            BlogCommands::List { space, limit } => {
                pages::list_blogposts(&ctx, space.as_deref(), limit).await
            }
            BlogCommands::Get { blogpost_id } => pages::get_blogpost(&ctx, &blogpost_id).await,
            BlogCommands::Create { space, title, body } => {
                pages::create_blog(&ctx, &space, &title, body.as_ref()).await
            }
            BlogCommands::Update {
                blogpost_id,
                title,
                body,
            } => pages::update_blogpost(&ctx, &blogpost_id, title.as_deref(), body.as_ref()).await,
            BlogCommands::Delete { blogpost_id, force } => {
                pages::delete_blogpost(&ctx, &blogpost_id, force).await
            }
        },
        ConfluenceCommands::Attachment(cmd) => match cmd {
            AttachmentCommands::List { page_id } => {
                attachments::list_attachments(&ctx, &page_id).await
            }
            AttachmentCommands::Get { attachment_id } => {
                attachments::get_attachment(&ctx, &attachment_id).await
            }
            AttachmentCommands::Upload {
                page_id,
                file,
                comment,
            } => attachments::upload_attachment(&ctx, &page_id, &file, comment.as_deref()).await,
            AttachmentCommands::Download {
                attachment_id,
                output,
            } => attachments::download_attachment(&ctx, &attachment_id, &output).await,
            AttachmentCommands::Delete {
                attachment_id,
                force,
            } => attachments::delete_attachment(&ctx, &attachment_id, force).await,
        },
        ConfluenceCommands::Search(cmd) => match cmd {
            SearchCommands::Cql { query, limit } => search::search_cql(&ctx, &query, limit).await,
            SearchCommands::Text { query, limit } => search::search_text(&ctx, &query, limit).await,
            SearchCommands::InSpace {
                space,
                query,
                limit,
            } => search::search_in_space(&ctx, &space, &query, limit).await,
            SearchCommands::Params {
                space,
                r#type,
                creator,
                label,
                title,
                text,
                show_query,
                limit,
            } => {
                search::search_params(
                    &ctx,
                    space.as_deref(),
                    r#type.as_deref(),
                    creator.as_deref(),
                    &label,
                    title.as_deref(),
                    text.as_deref(),
                    show_query,
                    limit,
                )
                .await
            }
        },
        ConfluenceCommands::Bulk(cmd) => match cmd {
            BulkCommands::Delete {
                cql,
                dry_run,
                concurrency,
            } => bulk::bulk_delete_pages(&ctx, &cql, dry_run, concurrency).await,
            BulkCommands::AddLabels {
                cql,
                labels,
                dry_run,
                concurrency,
            } => bulk::bulk_add_labels(&ctx, &cql, labels, dry_run, concurrency).await,
            BulkCommands::Export {
                cql,
                output,
                format,
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
                bulk::bulk_export_pages(&ctx, &cql, &output, export_format).await
            }
        },
        ConfluenceCommands::Analytics(cmd) => match cmd {
            AnalyticsCommands::PageViews { page_id, from } => {
                analytics::get_page_views(&ctx, &page_id, from.as_deref()).await
            }
            AnalyticsCommands::SpaceStats { space_key } => {
                analytics::get_space_analytics(&ctx, &space_key).await
            }
        },
    }
}
