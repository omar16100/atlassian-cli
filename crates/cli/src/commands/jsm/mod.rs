use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use clap::{Args, Subcommand};

mod approvals;
mod customers;
mod feedback;
mod knowledgebase;
mod organizations;
mod queues;
mod requests;
mod requesttypes;
mod servicedesk;
mod sla;
pub mod utils;

pub use utils::JsmContext;

#[derive(Args, Debug, Clone)]
pub struct JsmArgs {
    #[command(subcommand)]
    command: JsmCommands,
}

#[derive(Subcommand, Debug, Clone)]
enum JsmCommands {
    /// Service desk operations.
    #[command(subcommand)]
    ServiceDesk(ServiceDeskCommands),

    /// Customer request operations.
    #[command(subcommand)]
    Request(RequestCommands),

    /// Queue operations.
    #[command(subcommand)]
    Queue(QueueCommands),

    /// Approval operations.
    #[command(subcommand)]
    Approval(ApprovalCommands),

    /// SLA operations.
    #[command(subcommand)]
    Sla(SlaCommands),

    /// Customer operations.
    #[command(subcommand)]
    Customer(CustomerCommands),

    /// Organization operations.
    #[command(subcommand)]
    Organization(OrganizationCommands),

    /// Request type operations.
    #[command(subcommand)]
    RequestType(RequestTypeCommands),

    /// Knowledge base operations.
    #[command(subcommand)]
    Kb(KbCommands),

    /// Feedback operations.
    #[command(subcommand)]
    Feedback(FeedbackCommands),
}

#[derive(Subcommand, Debug, Clone)]
enum ServiceDeskCommands {
    /// List service desks available to the account.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get a single service desk by ID.
    Get { id: i64 },
    /// List customers of a service desk.
    Customers {
        id: i64,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Add customer to service desk.
    AddCustomer {
        id: i64,
        #[arg(long, required = true)]
        account_id: Vec<String>,
    },
    /// Remove customer from service desk.
    RemoveCustomer {
        id: i64,
        #[arg(long, required = true)]
        account_id: Vec<String>,
    },
    /// List organizations of a service desk.
    Organizations {
        id: i64,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Add organization to service desk.
    AddOrganization {
        id: i64,
        #[arg(long)]
        org_id: i64,
    },
    /// Remove organization from service desk.
    RemoveOrganization {
        id: i64,
        #[arg(long)]
        org_id: i64,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum RequestCommands {
    /// List requests, optionally filtered by service desk.
    List {
        #[arg(long)]
        servicedesk_id: Option<i64>,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get request details (issue key or ID).
    Get {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Create a new request.
    Create {
        #[arg(long)]
        servicedesk_id: i64,
        #[arg(long)]
        request_type_id: i64,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// List available transitions for a request.
    Transitions {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Transition a request to a new status.
    Transition {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long)]
        transition: String,
    },
    /// Get request status history.
    Status {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// List comments on a request.
    Comments {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Add comment to a request.
    AddComment {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long)]
        body: String,
        #[arg(long, default_value_t = false)]
        public: bool,
    },
    /// List participants of a request.
    Participants {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Add participant to a request.
    AddParticipant {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long, required = true)]
        account_id: Vec<String>,
    },
    /// Remove participant from a request.
    RemoveParticipant {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long, required = true)]
        account_id: Vec<String>,
    },
    /// Subscribe to request notifications.
    Subscribe {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Unsubscribe from request notifications.
    Unsubscribe {
        #[arg(value_name = "KEY")]
        key: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum QueueCommands {
    /// List queues for a service desk.
    List { servicedesk_id: i64 },
    /// Get queue details.
    Get { servicedesk_id: i64, queue_id: i64 },
    /// List issues in a queue.
    Issues {
        servicedesk_id: i64,
        queue_id: i64,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum ApprovalCommands {
    /// List approvals for a request.
    List {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Get approval details.
    Get {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long)]
        approval_id: i64,
    },
    /// Approve a request.
    Approve {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long)]
        approval_id: i64,
    },
    /// Decline a request.
    Decline {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long)]
        approval_id: i64,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum SlaCommands {
    /// List SLAs for a request.
    List {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Get specific SLA details.
    Get {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long)]
        sla_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum CustomerCommands {
    /// Create a customer.
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        display_name: String,
    },
    /// Revoke portal-only access for a user.
    RevokePortalAccess {
        #[arg(long)]
        account_id: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum OrganizationCommands {
    /// List all organizations.
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get organization details.
    Get { org_id: i64 },
    /// Create an organization.
    Create {
        #[arg(long)]
        name: String,
    },
    /// Delete an organization.
    Delete { org_id: i64 },
    /// List users in an organization.
    Users {
        org_id: i64,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Add users to an organization.
    AddUser {
        org_id: i64,
        #[arg(long, required = true)]
        account_id: Vec<String>,
    },
    /// Remove users from an organization.
    RemoveUser {
        org_id: i64,
        #[arg(long, required = true)]
        account_id: Vec<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum RequestTypeCommands {
    /// List all request types.
    List {
        #[arg(long)]
        servicedesk_id: Option<i64>,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Get request type details.
    Get { servicedesk_id: i64, type_id: i64 },
    /// List fields for a request type.
    Fields { servicedesk_id: i64, type_id: i64 },
    /// List request type groups for a service desk.
    Groups { servicedesk_id: i64 },
}

#[derive(Subcommand, Debug, Clone)]
enum KbCommands {
    /// Search knowledge base articles.
    Search {
        #[arg(long)]
        query: String,
        #[arg(long)]
        servicedesk_id: Option<i64>,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum FeedbackCommands {
    /// Get feedback for a request.
    Get {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Submit CSAT feedback for a request.
    Submit {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(long)]
        rating: i32,
        #[arg(long)]
        comment: Option<String>,
    },
    /// Delete feedback for a request.
    Delete {
        #[arg(value_name = "KEY")]
        key: String,
    },
}

/// Execute JSM command.
pub async fn execute(args: JsmArgs, client: ApiClient, renderer: &OutputRenderer) -> Result<()> {
    let ctx = JsmContext { client, renderer };

    match args.command {
        JsmCommands::ServiceDesk(cmd) => match cmd {
            ServiceDeskCommands::List { limit } => {
                servicedesk::list_service_desks(&ctx, limit).await
            }
            ServiceDeskCommands::Get { id } => servicedesk::get_service_desk(&ctx, id).await,
            ServiceDeskCommands::Customers { id, limit } => {
                servicedesk::list_customers(&ctx, id, limit).await
            }
            ServiceDeskCommands::AddCustomer { id, account_id } => {
                servicedesk::add_customer(&ctx, id, account_id).await
            }
            ServiceDeskCommands::RemoveCustomer { id, account_id } => {
                servicedesk::remove_customer(&ctx, id, account_id).await
            }
            ServiceDeskCommands::Organizations { id, limit } => {
                servicedesk::list_organizations(&ctx, id, limit).await
            }
            ServiceDeskCommands::AddOrganization { id, org_id } => {
                servicedesk::add_organization(&ctx, id, org_id).await
            }
            ServiceDeskCommands::RemoveOrganization { id, org_id } => {
                servicedesk::remove_organization(&ctx, id, org_id).await
            }
        },
        JsmCommands::Request(cmd) => match cmd {
            RequestCommands::List {
                servicedesk_id,
                limit,
            } => requests::list_requests(&ctx, servicedesk_id, limit).await,
            RequestCommands::Get { key } => requests::get_request(&ctx, &key).await,
            RequestCommands::Create {
                servicedesk_id,
                request_type_id,
                summary,
                description,
            } => {
                requests::create_request(
                    &ctx,
                    servicedesk_id,
                    request_type_id,
                    summary,
                    description,
                )
                .await
            }
            RequestCommands::Transitions { key } => requests::list_transitions(&ctx, &key).await,
            RequestCommands::Transition { key, transition } => {
                requests::transition_request(&ctx, &key, &transition).await
            }
            RequestCommands::Status { key } => requests::get_status_history(&ctx, &key).await,
            RequestCommands::Comments { key, limit } => {
                requests::list_comments(&ctx, &key, limit).await
            }
            RequestCommands::AddComment { key, body, public } => {
                requests::add_comment(&ctx, &key, body, public).await
            }
            RequestCommands::Participants { key } => requests::list_participants(&ctx, &key).await,
            RequestCommands::AddParticipant { key, account_id } => {
                requests::add_participant(&ctx, &key, account_id).await
            }
            RequestCommands::RemoveParticipant { key, account_id } => {
                requests::remove_participant(&ctx, &key, account_id).await
            }
            RequestCommands::Subscribe { key } => requests::subscribe(&ctx, &key).await,
            RequestCommands::Unsubscribe { key } => requests::unsubscribe(&ctx, &key).await,
        },
        JsmCommands::Queue(cmd) => match cmd {
            QueueCommands::List { servicedesk_id } => {
                queues::list_queues(&ctx, servicedesk_id).await
            }
            QueueCommands::Get {
                servicedesk_id,
                queue_id,
            } => queues::get_queue(&ctx, servicedesk_id, queue_id).await,
            QueueCommands::Issues {
                servicedesk_id,
                queue_id,
                limit,
            } => queues::list_queue_issues(&ctx, servicedesk_id, queue_id, limit).await,
        },
        JsmCommands::Approval(cmd) => match cmd {
            ApprovalCommands::List { key } => approvals::list_approvals(&ctx, &key).await,
            ApprovalCommands::Get { key, approval_id } => {
                approvals::get_approval(&ctx, &key, approval_id).await
            }
            ApprovalCommands::Approve { key, approval_id } => {
                approvals::answer_approval(&ctx, &key, approval_id, true).await
            }
            ApprovalCommands::Decline { key, approval_id } => {
                approvals::answer_approval(&ctx, &key, approval_id, false).await
            }
        },
        JsmCommands::Sla(cmd) => match cmd {
            SlaCommands::List { key } => sla::list_slas(&ctx, &key).await,
            SlaCommands::Get { key, sla_id } => sla::get_sla(&ctx, &key, &sla_id).await,
        },
        JsmCommands::Customer(cmd) => match cmd {
            CustomerCommands::Create {
                email,
                display_name,
            } => customers::create_customer(&ctx, email, display_name).await,
            CustomerCommands::RevokePortalAccess { account_id } => {
                customers::revoke_portal_access(&ctx, &account_id).await
            }
        },
        JsmCommands::Organization(cmd) => match cmd {
            OrganizationCommands::List { limit } => {
                organizations::list_organizations(&ctx, limit).await
            }
            OrganizationCommands::Get { org_id } => {
                organizations::get_organization(&ctx, org_id).await
            }
            OrganizationCommands::Create { name } => {
                organizations::create_organization(&ctx, name).await
            }
            OrganizationCommands::Delete { org_id } => {
                organizations::delete_organization(&ctx, org_id).await
            }
            OrganizationCommands::Users { org_id, limit } => {
                organizations::list_organization_users(&ctx, org_id, limit).await
            }
            OrganizationCommands::AddUser { org_id, account_id } => {
                organizations::add_organization_user(&ctx, org_id, account_id).await
            }
            OrganizationCommands::RemoveUser { org_id, account_id } => {
                organizations::remove_organization_user(&ctx, org_id, account_id).await
            }
        },
        JsmCommands::RequestType(cmd) => match cmd {
            RequestTypeCommands::List {
                servicedesk_id,
                limit,
            } => {
                if let Some(sd_id) = servicedesk_id {
                    requesttypes::list_request_types(&ctx, sd_id, limit).await
                } else {
                    requesttypes::list_all_request_types(&ctx, limit).await
                }
            }
            RequestTypeCommands::Get {
                servicedesk_id,
                type_id,
            } => requesttypes::get_request_type(&ctx, servicedesk_id, type_id).await,
            RequestTypeCommands::Fields {
                servicedesk_id,
                type_id,
            } => requesttypes::list_request_type_fields(&ctx, servicedesk_id, type_id).await,
            RequestTypeCommands::Groups { servicedesk_id } => {
                requesttypes::list_request_type_groups(&ctx, servicedesk_id).await
            }
        },
        JsmCommands::Kb(cmd) => match cmd {
            KbCommands::Search {
                query,
                servicedesk_id,
                limit,
            } => knowledgebase::search_articles(&ctx, &query, servicedesk_id, limit).await,
        },
        JsmCommands::Feedback(cmd) => match cmd {
            FeedbackCommands::Get { key } => feedback::get_feedback(&ctx, &key).await,
            FeedbackCommands::Submit {
                key,
                rating,
                comment,
            } => feedback::submit_feedback(&ctx, &key, rating, comment).await,
            FeedbackCommands::Delete { key } => feedback::delete_feedback(&ctx, &key).await,
        },
    }
}
