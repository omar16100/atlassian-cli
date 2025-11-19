use anyhow::{Context, Result};
use atlassiancli_api::ApiClient;
use atlassiancli_output::OutputRenderer;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

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
    Repo {
        #[command(subcommand)]
        command: RepoCommands,
    },

    /// Pull requests.
    Pr {
        #[command(subcommand)]
        command: PrCommands,
    },
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
}

#[derive(Subcommand, Debug, Clone)]
enum PrCommands {
    /// List pull requests for a repository.
    List {
        slug: String,
        #[arg(long, default_value = "OPEN")]
        state: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}

pub struct BitbucketContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}

pub async fn execute(args: BitbucketArgs, ctx: BitbucketContext<'_>) -> Result<()> {
    let workspace = args
        .workspace
        // .or_else(|| infer_workspace(ctx.client.base_url()))
        .ok_or_else(|| {
            anyhow::anyhow!("Workspace must be provided (--workspace) or inferred from base URL")
        })?;

    match args.command {
        BitbucketCommands::Repo { command } => match command {
            RepoCommands::List { limit } => list_repos(&ctx, &workspace, limit).await,
            RepoCommands::Get { slug } => get_repo(&ctx, &workspace, &slug).await,
        },
        BitbucketCommands::Pr { command } => match command {
            PrCommands::List { slug, state, limit } => {
                list_pull_requests(&ctx, &workspace, &slug, &state, limit).await
            }
        },
    }
}

#[allow(dead_code)]
fn infer_workspace(base_url: &str) -> Option<String> {
    let host = url::Url::parse(base_url).ok()?.host_str()?.to_string();
    if host.ends_with("bitbucket.org") {
        host.split('.').next().map(|s| s.to_string())
    } else {
        None
    }
}

async fn list_repos(ctx: &BitbucketContext<'_>, workspace: &str, limit: usize) -> Result<()> {
    #[derive(Deserialize)]
    struct RepoList {
        values: Vec<Repo>,
    }

    #[derive(Deserialize)]
    struct Repo {
        slug: String,
        name: Option<String>,
        #[serde(default)]
        is_private: bool,
        #[serde(default)]
        mainbranch: Option<BranchRef>,
    }

    #[derive(Deserialize)]
    struct BranchRef {
        name: String,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("pagelen", &limit.min(100).to_string())
        .finish();
    let path = format!("/2.0/repositories/{workspace}?{query}");

    let response: RepoList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list repositories for workspace {workspace}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        slug: &'a str,
        name: &'a str,
        main_branch: &'a str,
        visibility: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|repo| Row {
            slug: repo.slug.as_str(),
            name: repo.name.as_deref().unwrap_or(""),
            main_branch: repo
                .mainbranch
                .as_ref()
                .map(|b| b.name.as_str())
                .unwrap_or(""),
            visibility: if repo.is_private { "private" } else { "public" },
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(
            workspace,
            "No repositories returned for workspace; check permissions."
        );
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

async fn get_repo(ctx: &BitbucketContext<'_>, workspace: &str, slug: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Repo {
        slug: String,
        name: Option<String>,
        #[serde(rename = "full_name")]
        full_name: Option<String>,
        description: Option<String>,
        #[serde(default)]
        is_private: bool,
        #[serde(default)]
        mainbranch: Option<BranchRef>,
    }

    #[derive(Deserialize)]
    struct BranchRef {
        name: String,
    }

    let path = format!("/2.0/repositories/{workspace}/{slug}");
    let repo: Repo = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch repository {workspace}/{slug}"))?;

    #[derive(Serialize)]
    struct View<'a> {
        slug: &'a str,
        name: &'a str,
        full_name: &'a str,
        description: &'a str,
        main_branch: &'a str,
        visibility: &'a str,
    }

    let view = View {
        slug: repo.slug.as_str(),
        name: repo.name.as_deref().unwrap_or(""),
        full_name: repo.full_name.as_deref().unwrap_or(""),
        description: repo.description.as_deref().unwrap_or(""),
        main_branch: repo
            .mainbranch
            .as_ref()
            .map(|b| b.name.as_str())
            .unwrap_or(""),
        visibility: if repo.is_private { "private" } else { "public" },
    };

    ctx.renderer.render(&view)
}

async fn list_pull_requests(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    slug: &str,
    state: &str,
    limit: usize,
) -> Result<()> {
    #[derive(Deserialize)]
    struct PullRequestList {
        values: Vec<PullRequest>,
    }

    #[derive(Deserialize)]
    struct PullRequest {
        id: i64,
        title: String,
        state: String,
        author: User,
        source: PullRequestBranch,
        destination: PullRequestBranch,
    }

    #[derive(Deserialize)]
    struct User {
        display_name: String,
    }

    #[derive(Deserialize)]
    struct PullRequestBranch {
        branch: BranchRef,
    }

    #[derive(Deserialize)]
    struct BranchRef {
        name: String,
    }

    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("state", state)
        .append_pair("pagelen", &limit.min(100).to_string())
        .finish();
    let path = format!("/2.0/repositories/{workspace}/{slug}/pullrequests?{query}");

    let response: PullRequestList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list pull requests for {workspace}/{slug}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        title: &'a str,
        state: &'a str,
        author: &'a str,
        source: &'a str,
        destination: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|pr| Row {
            id: pr.id,
            title: pr.title.as_str(),
            state: pr.state.as_str(),
            author: pr.author.display_name.as_str(),
            source: pr.source.branch.name.as_str(),
            destination: pr.destination.branch.name.as_str(),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, slug, "No pull requests returned for repository");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}
