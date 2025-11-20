use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::BitbucketContext;

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
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    created_on: Option<String>,
    #[serde(default)]
    updated_on: Option<String>,
    #[serde(default)]
    comment_count: Option<i32>,
    #[serde(default)]
    task_count: Option<i32>,
    #[serde(default)]
    participants: Option<Vec<Participant>>,
}

#[derive(Deserialize)]
struct User {
    display_name: String,
    #[allow(dead_code)]
    #[serde(default)]
    uuid: Option<String>,
}

#[derive(Deserialize)]
struct PullRequestBranch {
    branch: BranchRef,
    #[allow(dead_code)]
    #[serde(default)]
    repository: Option<Repository>,
}

#[derive(Deserialize)]
struct BranchRef {
    name: String,
}

#[derive(Deserialize)]
struct Repository {
    #[allow(dead_code)]
    #[serde(default)]
    full_name: Option<String>,
}

#[derive(Deserialize)]
struct Participant {
    #[allow(dead_code)]
    #[serde(default)]
    approved: bool,
    #[allow(dead_code)]
    user: User,
    #[allow(dead_code)]
    role: String,
}

#[derive(Deserialize)]
struct Comment {
    id: i64,
    content: CommentContent,
    user: User,
    #[serde(default)]
    created_on: Option<String>,
}

#[derive(Deserialize)]
struct CommentContent {
    raw: String,
}

pub async fn list_pull_requests(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    slug: &str,
    state: &str,
    limit: usize,
) -> Result<()> {
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

pub async fn get_pull_request(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}");
    let pr: PullRequest = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to fetch pull request {pr_id} from {workspace}/{repo_slug}")
    })?;

    #[derive(Serialize)]
    struct View<'a> {
        id: i64,
        title: &'a str,
        state: &'a str,
        author: &'a str,
        source: &'a str,
        destination: &'a str,
        description: &'a str,
        created: &'a str,
        updated: &'a str,
        comments: String,
        tasks: String,
        approvals: String,
    }

    let approvals = pr
        .participants
        .as_ref()
        .map(|p| p.iter().filter(|part| part.approved).count())
        .unwrap_or(0);

    let view = View {
        id: pr.id,
        title: pr.title.as_str(),
        state: pr.state.as_str(),
        author: pr.author.display_name.as_str(),
        source: pr.source.branch.name.as_str(),
        destination: pr.destination.branch.name.as_str(),
        description: pr.description.as_deref().unwrap_or(""),
        created: pr.created_on.as_deref().unwrap_or(""),
        updated: pr.updated_on.as_deref().unwrap_or(""),
        comments: pr.comment_count.map(|c| c.to_string()).unwrap_or_default(),
        tasks: pr.task_count.map(|t| t.to_string()).unwrap_or_default(),
        approvals: approvals.to_string(),
    };

    ctx.renderer.render(&view)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_pull_request(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    title: &str,
    source_branch: &str,
    dest_branch: &str,
    description: Option<&str>,
    reviewers: Vec<String>,
) -> Result<()> {
    let mut payload = serde_json::json!({
        "title": title,
        "source": {
            "branch": {
                "name": source_branch
            }
        },
        "destination": {
            "branch": {
                "name": dest_branch
            }
        }
    });

    if let Some(desc) = description {
        payload["description"] = serde_json::json!(desc);
    }

    if !reviewers.is_empty() {
        let reviewer_objs: Vec<_> = reviewers
            .iter()
            .map(|uuid| serde_json::json!({"uuid": uuid}))
            .collect();
        payload["reviewers"] = serde_json::json!(reviewer_objs);
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests");
    let pr: PullRequest = ctx
        .client
        .post(&path, &payload)
        .await
        .with_context(|| format!("Failed to create pull request in {workspace}/{repo_slug}"))?;

    tracing::info!(
        pr_id = pr.id,
        workspace,
        repo_slug,
        "Pull request created successfully"
    );

    #[derive(Serialize)]
    struct Created {
        id: i64,
        title: String,
        source: String,
        destination: String,
        state: String,
    }

    let created = Created {
        id: pr.id,
        title: pr.title.clone(),
        source: pr.source.branch.name.clone(),
        destination: pr.destination.branch.name.clone(),
        state: pr.state.clone(),
    };

    ctx.renderer.render(&created)
}

pub async fn update_pull_request(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    title: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    let mut payload = serde_json::json!({});

    if let Some(t) = title {
        payload["title"] = serde_json::json!(t);
    }

    if let Some(d) = description {
        payload["description"] = serde_json::json!(d);
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}");
    let pr: PullRequest = ctx.client.put(&path, &payload).await.with_context(|| {
        format!("Failed to update pull request {pr_id} in {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        pr_id = pr.id,
        workspace,
        repo_slug,
        "Pull request updated successfully"
    );

    #[derive(Serialize)]
    struct Updated {
        id: i64,
        title: String,
        description: String,
        state: String,
    }

    let updated = Updated {
        id: pr.id,
        title: pr.title.clone(),
        description: pr.description.unwrap_or_default(),
        state: pr.state.clone(),
    };

    ctx.renderer.render(&updated)
}

pub async fn merge_pull_request(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    merge_strategy: Option<&str>,
    message: Option<&str>,
) -> Result<()> {
    let mut payload = serde_json::json!({});

    if let Some(strategy) = merge_strategy {
        payload["merge_strategy"] = serde_json::json!(strategy);
    }

    if let Some(msg) = message {
        payload["message"] = serde_json::json!(msg);
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/merge");
    let pr: PullRequest = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to merge pull request {pr_id} in {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        pr_id = pr.id,
        workspace,
        repo_slug,
        "Pull request merged successfully"
    );

    #[derive(Serialize)]
    struct Merged {
        id: i64,
        title: String,
        state: String,
        source: String,
        destination: String,
    }

    let merged = Merged {
        id: pr.id,
        title: pr.title.clone(),
        state: pr.state.clone(),
        source: pr.source.branch.name.clone(),
        destination: pr.destination.branch.name.clone(),
    };

    ctx.renderer.render(&merged)
}

pub async fn decline_pull_request(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/decline");
    let pr: PullRequest = ctx
        .client
        .post(&path, &serde_json::json!({}))
        .await
        .with_context(|| {
            format!("Failed to decline pull request {pr_id} in {workspace}/{repo_slug}")
        })?;

    tracing::info!(
        pr_id = pr.id,
        workspace,
        repo_slug,
        "Pull request declined successfully"
    );

    println!("✓ Pull request #{pr_id} declined: {}", pr.title);
    Ok(())
}

pub async fn approve_pull_request(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
) -> Result<()> {
    #[derive(Deserialize)]
    struct Approval {
        #[allow(dead_code)]
        #[serde(default)]
        approved: bool,
        #[allow(dead_code)]
        user: User,
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/approve");
    let approval: Approval = ctx
        .client
        .post(&path, &serde_json::json!({}))
        .await
        .with_context(|| {
            format!("Failed to approve pull request {pr_id} in {workspace}/{repo_slug}")
        })?;

    tracing::info!(
        pr_id,
        workspace,
        repo_slug,
        "Pull request approved successfully"
    );

    println!(
        "✓ Pull request #{pr_id} approved by {}",
        approval.user.display_name
    );
    Ok(())
}

pub async fn unapprove_pull_request(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/approve");
    let _: serde_json::Value = ctx.client.delete(&path).await.with_context(|| {
        format!("Failed to unapprove pull request {pr_id} in {workspace}/{repo_slug}")
    })?;

    tracing::info!(
        pr_id,
        workspace,
        repo_slug,
        "Pull request approval removed successfully"
    );

    println!("✓ Approval removed from pull request #{pr_id}");
    Ok(())
}

pub async fn list_pr_comments(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
) -> Result<()> {
    #[derive(Deserialize)]
    struct CommentList {
        values: Vec<Comment>,
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments");
    let response: CommentList = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to list comments for pull request {pr_id} in {workspace}/{repo_slug}")
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        author: &'a str,
        content: &'a str,
        created: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|comment| Row {
            id: comment.id,
            author: comment.user.display_name.as_str(),
            content: comment.content.raw.lines().next().unwrap_or(""),
            created: comment.created_on.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(pr_id, workspace, repo_slug, "No comments on pull request");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn add_pr_comment(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    content: &str,
) -> Result<()> {
    let payload = serde_json::json!({
        "content": {
            "raw": content
        }
    });

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments");
    let comment: Comment = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to add comment to pull request {pr_id} in {workspace}/{repo_slug}")
    })?;

    tracing::info!(comment_id = comment.id, pr_id, "Comment added successfully");

    println!("✓ Comment added to pull request #{pr_id}");
    Ok(())
}

pub async fn add_pr_reviewers(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    reviewers: Vec<String>,
) -> Result<()> {
    for uuid in reviewers {
        let path = format!(
            "/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/default-reviewers/{uuid}"
        );
        let _: serde_json::Value = ctx
            .client
            .put(&path, &serde_json::json!({}))
            .await
            .with_context(|| format!("Failed to add reviewer {uuid} to pull request {pr_id}"))?;

        tracing::info!(uuid, pr_id, "Reviewer added successfully");
    }

    println!("✓ Reviewers added to pull request #{pr_id}");
    Ok(())
}

pub async fn get_pr_diff(
    _ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
) -> Result<()> {
    tracing::info!(
        pr_id,
        workspace,
        repo_slug,
        "Fetching diff for pull request"
    );

    println!("Diff for pull request #{pr_id}:");
    println!("View at: https://bitbucket.org/{workspace}/{repo_slug}/pull-requests/{pr_id}/diff");
    println!("\nNote: Use the web interface to view the full diff with syntax highlighting");

    Ok(())
}
