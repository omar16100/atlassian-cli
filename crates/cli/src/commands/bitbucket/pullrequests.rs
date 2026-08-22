use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::BitbucketContext;
use crate::commands::common::{render_success, MutationResult};

/// Information about a pull request for pipeline operations
#[derive(Debug, Clone)]
pub struct PullRequestInfo {
    pub source_branch: String,
    pub source_workspace: String,
    pub source_repo: String,
    pub state: String,
}

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
    #[serde(default)]
    reviewers: Option<Vec<User>>,
}

#[derive(Deserialize)]
struct User {
    display_name: String,
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

#[derive(Deserialize, Clone)]
struct Repository {
    #[allow(dead_code)]
    #[serde(default)]
    full_name: Option<String>,
    #[serde(default)]
    workspace: Option<RepositoryWorkspace>,
    name: String,
}

#[derive(Deserialize, Clone)]
struct RepositoryWorkspace {
    slug: String,
}

#[derive(Deserialize)]
struct Participant {
    #[serde(default)]
    approved: bool,
    user: User,
    role: String,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    participated_on: Option<String>,
}

/// Derive a human-friendly review status label from a participant's `approved`/`state` fields.
fn participant_status(approved: bool, state: Option<&str>) -> &'static str {
    match state {
        Some("approved") => "Approved",
        Some("changes_requested") => "Changes Requested",
        _ if approved => "Approved",
        _ => "No Response",
    }
}

#[derive(Serialize)]
struct ReviewerRow<'a> {
    name: &'a str,
    role: &'a str,
    status: &'a str,
    participated_on: &'a str,
}

/// Build the reviewer table rows. Only `role == REVIEWER` participants are kept unless
/// `show_all` is set, in which case commenters and other participants are included too.
fn reviewer_rows(participants: &[Participant], show_all: bool) -> Vec<ReviewerRow<'_>> {
    participants
        .iter()
        .filter(|p| show_all || p.role == "REVIEWER")
        .map(|p| ReviewerRow {
            name: p.user.display_name.as_str(),
            role: p.role.as_str(),
            status: participant_status(p.approved, p.state.as_deref()),
            participated_on: p.participated_on.as_deref().unwrap_or(""),
        })
        .collect()
}

#[derive(Deserialize)]
struct Comment {
    id: i64,
    content: CommentContent,
    user: User,
    #[serde(default)]
    created_on: Option<String>,
    #[serde(default)]
    parent: Option<CommentParent>,
    #[serde(default)]
    inline: Option<Inline>,
}

#[derive(Deserialize)]
struct CommentContent {
    raw: String,
}

#[derive(Deserialize)]
struct CommentParent {
    id: i64,
}

/// Inline anchor metadata returned by Bitbucket for a PR comment.
///
/// Present iff the comment is inline (attached to a file). `to` and `from`
/// are line numbers in the destination and source revisions respectively;
/// exactly one is set for a line-anchored comment, and both are `None` for
/// a file-level inline comment.
#[derive(Deserialize, Debug, Clone)]
struct Inline {
    #[serde(default)]
    path: String,
    #[serde(default)]
    to: Option<u32>,
    #[serde(default)]
    from: Option<u32>,
}

/// Render an inline anchor as a compact `path:line`, or `path` alone for a
/// file-level comment, or `""` for a top-level one.
///
/// A `to`/`from` of `0` is treated as "no line", to stay consistent with the
/// `--line >= 1` invariant: Bitbucket line numbers are 1-indexed, so a 0 in a
/// response is a sentinel or a mis-serialised field, not a real anchor.
fn format_comment_location(inline: Option<&Inline>) -> String {
    match inline {
        None => String::new(),
        Some(i) => match i.to.filter(|&n| n > 0).or(i.from.filter(|&n| n > 0)) {
            Some(line) => format!("{}:{}", i.path, line),
            None => i.path.clone(),
        },
    }
}

/// Which side of the diff a line-anchored inline comment attaches to.
///
/// - `New` (default): the destination revision, i.e. an added or unchanged
///   line. Sent as Bitbucket's `inline.to`.
/// - `Old`: the source revision, i.e. a removed line. Sent as `inline.from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Side {
    #[default]
    #[value(name = "new")]
    New,
    #[value(name = "old")]
    Old,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::New => f.write_str("new"),
            Side::Old => f.write_str("old"),
        }
    }
}

#[derive(Serialize)]
struct CommentRow<'a> {
    id: i64,
    author: &'a str,
    content: &'a str,
    created: &'a str,
    parent: Option<i64>,
    /// Appended rather than inserted: JSON key order, CSV columns and table
    /// headers stay additive for anything already parsing this output.
    location: String,
}

fn comment_row(comment: &Comment) -> CommentRow<'_> {
    CommentRow {
        id: comment.id,
        author: comment.user.display_name.as_str(),
        content: comment.content.raw.as_str(),
        created: comment.created_on.as_deref().unwrap_or(""),
        parent: comment.parent.as_ref().map(|parent| parent.id),
        location: format_comment_location(comment.inline.as_ref()),
    }
}

/// Build the JSON body for a PR-comment POST.
///
/// Pure, so every shape is unit-testable without a mock server: top-level,
/// threaded reply, whole-file inline, and line-anchored inline on either side.
///
/// - `parent`: reply inside an existing thread.
/// - `inline_path`: makes the comment inline, i.e. attached to a file.
/// - `inline_line`: only meaningful with a path. Absent means a file-level
///   inline comment.
/// - `side`: only meaningful with a line. `New` sends `inline.to`, `Old` sends
///   `inline.from`.
fn comment_payload(
    content: &str,
    parent: Option<i64>,
    inline_path: Option<&str>,
    inline_line: Option<u32>,
    side: Side,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "content": {
            "raw": content
        }
    });
    if let Some(parent) = parent {
        payload["parent"] = serde_json::json!({"id": parent});
    }
    if let Some(path) = inline_path {
        let mut inline = serde_json::json!({ "path": path });
        if let Some(line) = inline_line {
            match side {
                Side::New => inline["to"] = serde_json::json!(line),
                Side::Old => inline["from"] = serde_json::json!(line),
            }
        }
        payload["inline"] = inline;
    }
    payload
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
        ctx.verify_auth().await?;
        tracing::info!(workspace, slug, "No pull requests found");
    }

    ctx.renderer
        .render_list_or_empty(&rows, "No pull requests found")
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
        let reviewer_objs: Vec<_> = merge_reviewer_uuids(&[], &reviewers)
            .iter()
            .map(|uuid| serde_json::json!({ "uuid": uuid }))
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

    render_success(
        ctx.renderer,
        &format!("✅ Pull request #{pr_id} declined: {}", pr.title),
        &MutationResult::with_id(format!("Pull request #{pr_id} declined"), pr_id.to_string()),
    )
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
        "✅ Pull request #{pr_id} approved by {}",
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

    render_success(
        ctx.renderer,
        &format!("✅ Approval removed from pull request #{pr_id}"),
        &MutationResult::with_id(
            format!("Approval removed from pull request #{pr_id}"),
            pr_id.to_string(),
        ),
    )
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

    let rows: Vec<CommentRow<'_>> = response.values.iter().map(comment_row).collect();

    if rows.is_empty() {
        tracing::info!(pr_id, workspace, repo_slug, "No comments found");
    }

    ctx.renderer
        .render_list_or_empty(&rows, "No comments found")
}

#[allow(clippy::too_many_arguments)]
pub async fn add_pr_comment(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    content: &str,
    parent: Option<i64>,
    inline_path: Option<&str>,
    inline_line: Option<u32>,
    side: Side,
) -> Result<()> {
    let payload = comment_payload(content, parent, inline_path, inline_line, side);

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments");
    let comment: Comment = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to add comment to pull request {pr_id} in {workspace}/{repo_slug}")
    })?;

    let is_inline = inline_path.is_some();
    tracing::info!(
        comment_id = comment.id,
        pr_id,
        is_inline,
        "Comment added successfully"
    );

    let kind = if is_inline {
        "Inline comment"
    } else {
        "Comment"
    };
    let message = format!("{kind} added to pull request #{pr_id}");
    render_success(
        ctx.renderer,
        &format!("✅ {message}"),
        &MutationResult::with_id(message.clone(), pr_id.to_string()),
    )
}

pub async fn resolve_pr_comment(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    comment_id: i64,
) -> Result<()> {
    let path = format!(
        "/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments/{comment_id}/resolve"
    );
    let _: serde_json::Value = ctx
        .client
        .post(&path, &serde_json::json!({}))
        .await
        .with_context(|| {
            format!(
                "Failed to resolve comment {comment_id} on pull request {pr_id} in {workspace}/{repo_slug}"
            )
        })?;

    tracing::info!(comment_id, pr_id, "Comment thread resolved successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Comment {comment_id} resolved on pull request #{pr_id}"),
        &MutationResult::with_id(
            format!("Comment {comment_id} resolved on pull request #{pr_id}"),
            comment_id.to_string(),
        ),
    )
}

pub async fn reopen_pr_comment(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    comment_id: i64,
) -> Result<()> {
    let path = format!(
        "/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments/{comment_id}/resolve"
    );
    ctx.client
        .delete_no_content(&path)
        .await
        .with_context(|| {
            format!(
                "Failed to reopen comment {comment_id} on pull request {pr_id} in {workspace}/{repo_slug}"
            )
        })?;

    tracing::info!(comment_id, pr_id, "Comment thread reopened successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Comment {comment_id} reopened on pull request #{pr_id}"),
        &MutationResult::with_id(
            format!("Comment {comment_id} reopened on pull request #{pr_id}"),
            comment_id.to_string(),
        ),
    )
}

/// Normalise a Bitbucket account UUID to the brace form the API expects (`{uuid}`),
/// so `--add abc-123` and `--add '{abc-123}'` both work.
fn normalize_uuid(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        trimmed.to_string()
    } else {
        format!("{{{trimmed}}}")
    }
}

/// Merge the PR's existing reviewer UUIDs with the requested ones, preserving order and
/// dropping duplicates and empties.
fn merge_reviewer_uuids(existing: &[String], requested: &[String]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    for uuid in existing.iter().chain(requested.iter()) {
        let normalized = normalize_uuid(uuid);
        if normalized == "{}" || merged.contains(&normalized) {
            continue;
        }
        merged.push(normalized);
    }
    merged
}

/// Add reviewers to a pull request.
///
/// Bitbucket Cloud has no endpoint for adding a single reviewer to an existing PR: the
/// reviewer list is replaced wholesale by a `PUT` on the pull request itself. So we read the
/// current reviewers, union the requested UUIDs in, and PUT the result back.
pub async fn add_pr_reviewers(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    reviewers: Vec<String>,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}");
    let pr: PullRequest = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to fetch pull request {pr_id} from {workspace}/{repo_slug}")
    })?;

    let existing: Vec<String> = pr
        .reviewers
        .as_ref()
        .map(|list| {
            list.iter()
                .filter_map(|user| user.uuid.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let merged = merge_reviewer_uuids(&existing, &reviewers);
    tracing::info!(
        pr_id,
        workspace,
        repo_slug,
        existing = existing.len(),
        requested = reviewers.len(),
        total = merged.len(),
        "Updating pull request reviewers"
    );

    let payload = serde_json::json!({
        "title": pr.title,
        "reviewers": merged
            .iter()
            .map(|uuid| serde_json::json!({ "uuid": uuid }))
            .collect::<Vec<_>>(),
    });

    let _: serde_json::Value = ctx
        .client
        .put(&path, &payload)
        .await
        .with_context(|| format!("Failed to add reviewers to pull request {pr_id}"))?;

    tracing::info!(pr_id, count = merged.len(), "Reviewers added successfully");

    render_success(
        ctx.renderer,
        &format!("✅ Reviewers added to pull request #{pr_id}"),
        &MutationResult::with_id(
            format!("Reviewers added to pull request #{pr_id}"),
            pr_id.to_string(),
        ),
    )
}

/// List reviewers (or all participants) on a pull request along with their review status.
pub async fn list_pr_reviewers(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    show_all: bool,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}");
    let pr: PullRequest = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to fetch pull request {pr_id} from {workspace}/{repo_slug}")
    })?;

    let participants = pr.participants.unwrap_or_default();
    let rows = reviewer_rows(&participants, show_all);

    if rows.is_empty() {
        tracing::info!(pr_id, workspace, repo_slug, show_all, "No reviewers found");
    }

    ctx.renderer
        .render_list_or_empty(&rows, "No reviewers found")
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

/// Get pull request information for pipeline operations
/// Returns source branch, workspace, repo, and PR state
pub async fn get_pr_info(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
) -> Result<PullRequestInfo> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}");
    let pr: PullRequest = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch PR #{pr_id} from {workspace}/{repo_slug}"))?;

    let source_branch = pr.source.branch.name;

    // Extract source workspace and repo from the source repository
    let (source_workspace, source_repo) = if let Some(ref repo) = pr.source.repository {
        let ws = repo
            .workspace
            .as_ref()
            .map(|w| w.slug.clone())
            .unwrap_or_else(|| workspace.to_string());
        let repo_name = repo.name.clone();
        (ws, repo_name)
    } else {
        // If no source repository info, assume same workspace/repo (not a fork)
        (workspace.to_string(), repo_slug.to_string())
    };

    Ok(PullRequestInfo {
        source_branch,
        source_workspace,
        source_repo,
        state: pr.state,
    })
}

/// Check if a PR is from a fork
pub fn is_from_fork(pr_info: &PullRequestInfo, target_workspace: &str, target_repo: &str) -> bool {
    pr_info.source_workspace != target_workspace || pr_info.source_repo != target_repo
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlassian_cli_api::ApiClient;
    use atlassian_cli_output::{OutputFormat, OutputRenderer};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_participant_status_approved_state() {
        assert_eq!(participant_status(true, Some("approved")), "Approved");
    }

    #[test]
    fn test_participant_status_changes_requested() {
        assert_eq!(
            participant_status(false, Some("changes_requested")),
            "Changes Requested"
        );
    }

    #[test]
    fn test_participant_status_no_state_but_approved_fallback() {
        // Older API responses may omit `state` but still set `approved`.
        assert_eq!(participant_status(true, None), "Approved");
    }

    #[test]
    fn test_participant_status_no_response() {
        assert_eq!(participant_status(false, None), "No Response");
    }

    fn participant(
        name: &str,
        role: &str,
        approved: bool,
        state: Option<&str>,
        participated_on: Option<&str>,
    ) -> Participant {
        Participant {
            approved,
            user: User {
                display_name: name.to_string(),
                uuid: None,
            },
            role: role.to_string(),
            state: state.map(str::to_string),
            participated_on: participated_on.map(str::to_string),
        }
    }

    fn sample_participants() -> Vec<Participant> {
        vec![
            participant(
                "Jane Doe",
                "REVIEWER",
                true,
                Some("approved"),
                Some("2026-08-10T12:00:00Z"),
            ),
            participant(
                "John Smith",
                "REVIEWER",
                false,
                Some("changes_requested"),
                Some("2026-08-11T09:30:00Z"),
            ),
            participant("Alex Lee", "REVIEWER", false, None, None),
            participant(
                "Sam Chen",
                "PARTICIPANT",
                false,
                None,
                Some("2026-08-12T08:00:00Z"),
            ),
        ]
    }

    #[test]
    fn test_reviewer_rows_default_excludes_participants() {
        let participants = sample_participants();
        let rows = reviewer_rows(&participants, false);

        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.role == "REVIEWER"));
        assert_eq!(rows[0].name, "Jane Doe");
        assert_eq!(rows[0].status, "Approved");
        assert_eq!(rows[0].participated_on, "2026-08-10T12:00:00Z");
        assert_eq!(rows[1].status, "Changes Requested");
        assert_eq!(rows[2].status, "No Response");
        // Missing participated_on renders as an empty cell, not "null".
        assert_eq!(rows[2].participated_on, "");
    }

    #[test]
    fn test_reviewer_rows_all_includes_participants() {
        let participants = sample_participants();
        let rows = reviewer_rows(&participants, true);

        assert_eq!(rows.len(), 4);
        assert_eq!(rows[3].name, "Sam Chen");
        assert_eq!(rows[3].role, "PARTICIPANT");
        assert_eq!(rows[3].status, "No Response");
    }

    #[test]
    fn test_reviewer_rows_empty() {
        assert!(reviewer_rows(&[], false).is_empty());
        assert!(reviewer_rows(&[], true).is_empty());
    }

    #[test]
    fn test_reviewer_rows_no_reviewers_only_participants() {
        let participants = vec![participant("Sam Chen", "PARTICIPANT", false, None, None)];

        assert!(reviewer_rows(&participants, false).is_empty());
        assert_eq!(reviewer_rows(&participants, true).len(), 1);
    }

    #[test]
    fn test_normalize_uuid_adds_braces() {
        assert_eq!(normalize_uuid("abc-123"), "{abc-123}");
        assert_eq!(normalize_uuid("{abc-123}"), "{abc-123}");
        assert_eq!(normalize_uuid("  abc-123  "), "{abc-123}");
    }

    #[test]
    fn test_merge_reviewer_uuids_unions_and_dedupes() {
        let existing = vec!["{a}".to_string(), "b".to_string()];
        let requested = vec!["b".to_string(), "{c}".to_string(), "a".to_string()];

        assert_eq!(
            merge_reviewer_uuids(&existing, &requested),
            vec!["{a}".to_string(), "{b}".to_string(), "{c}".to_string()]
        );
    }

    #[test]
    fn test_merge_reviewer_uuids_keeps_existing_when_nothing_requested() {
        let existing = vec!["{a}".to_string()];

        assert_eq!(
            merge_reviewer_uuids(&existing, &[]),
            vec!["{a}".to_string()]
        );
    }

    #[test]
    fn test_merge_reviewer_uuids_skips_empty_entries() {
        let requested = vec!["".to_string(), "  ".to_string(), "a".to_string()];

        assert_eq!(
            merge_reviewer_uuids(&[], &requested),
            vec!["{a}".to_string()]
        );
    }

    #[test]
    fn comment_payload_includes_parent_for_threaded_reply() {
        assert_eq!(
            comment_payload("Fixed", Some(843649259), None, None, Side::New),
            serde_json::json!({
                "content": {"raw": "Fixed"},
                "parent": {"id": 843649259}
            })
        );
        assert_eq!(
            comment_payload("Top level", None, None, None, Side::New),
            serde_json::json!({"content": {"raw": "Top level"}})
        );
    }

    #[test]
    fn comment_row_preserves_full_content_and_parent() {
        let mut comment: Comment = serde_json::from_value(serde_json::json!({
            "id": 843649260,
            "content": {"raw": "First line\nIdempotency marker"},
            "user": {"display_name": "OCR"},
            "created_on": "2026-08-18T06:01:54Z",
            "parent": {"id": 843649259}
        }))
        .unwrap();

        let row = serde_json::to_value(comment_row(&comment)).unwrap();
        assert_eq!(row["content"], "First line\nIdempotency marker");
        assert_eq!(row["parent"], 843649259);

        comment.parent = None;
        assert!(serde_json::to_value(comment_row(&comment)).unwrap()["parent"].is_null());
    }

    #[tokio::test]
    async fn comment_threads_can_be_resolved_and_reopened() {
        let server = MockServer::start().await;
        let endpoint = "/2.0/repositories/workspace/repo/pullrequests/42/comments/99/resolve";

        Mock::given(method("POST"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type": "pullrequest_comment_resolution"
            })))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = BitbucketContext {
            client: ApiClient::new(server.uri()).unwrap(),
            renderer: &renderer,
            is_bearer: false,
        };

        resolve_pr_comment(&ctx, "workspace", "repo", 42, 99)
            .await
            .unwrap();
        reopen_pr_comment(&ctx, "workspace", "repo", 42, 99)
            .await
            .unwrap();
    }

    #[test]
    fn test_side_display() {
        assert_eq!(Side::New.to_string(), "new");
        assert_eq!(Side::Old.to_string(), "old");
    }

    #[test]
    fn test_build_comment_payload_global() {
        let payload = comment_payload("Looks good!", None, None, None, Side::New);
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Looks good!" }
            })
        );
    }

    #[test]
    fn test_build_comment_payload_inline_new_side_line() {
        let payload = comment_payload(
            "Nit: rename this",
            None,
            Some("src/main.rs"),
            Some(42),
            Side::New,
        );
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Nit: rename this" },
                "inline": { "path": "src/main.rs", "to": 42 }
            })
        );
    }

    #[test]
    fn test_build_comment_payload_inline_old_side_line() {
        let payload = comment_payload(
            "Why remove this?",
            None,
            Some("src/main.rs"),
            Some(17),
            Side::Old,
        );
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Why remove this?" },
                "inline": { "path": "src/main.rs", "from": 17 }
            })
        );
    }

    #[test]
    fn test_build_comment_payload_file_level_inline_no_line() {
        // path but no line -> file-level inline comment; side is irrelevant and must not leak into payload
        let payload = comment_payload(
            "Whole-file comment",
            None,
            Some("README.md"),
            None,
            Side::New,
        );
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Whole-file comment" },
                "inline": { "path": "README.md" }
            })
        );
    }

    #[test]
    fn test_build_comment_payload_file_level_inline_ignores_side_when_no_line() {
        // Regression guard: with no line, `side` must not leak into the payload
        // regardless of whether it's New or Old.
        let new_side = comment_payload(
            "Whole-file comment",
            None,
            Some("README.md"),
            None,
            Side::New,
        );
        let old_side = comment_payload(
            "Whole-file comment",
            None,
            Some("README.md"),
            None,
            Side::Old,
        );
        assert_eq!(new_side, old_side);
        assert_eq!(
            old_side,
            serde_json::json!({
                "content": { "raw": "Whole-file comment" },
                "inline": { "path": "README.md" }
            })
        );
    }

    #[test]
    fn test_side_default_is_new() {
        assert_eq!(Side::default(), Side::New);
    }

    #[test]
    fn test_comment_deserializes_inline_line_new_side() {
        let raw = serde_json::json!({
            "id": 1,
            "content": { "raw": "nit" },
            "user": { "display_name": "Alice" },
            "created_on": "2026-08-20T12:00:00Z",
            "inline": { "path": "src/main.rs", "to": 42, "from": null }
        });
        let c: Comment = serde_json::from_value(raw).unwrap();
        let inline = c.inline.expect("inline present");
        assert_eq!(inline.path, "src/main.rs");
        assert_eq!(inline.to, Some(42));
        assert_eq!(inline.from, None);
    }

    #[test]
    fn test_comment_deserializes_inline_line_old_side() {
        let raw = serde_json::json!({
            "id": 2,
            "content": { "raw": "removed on purpose" },
            "user": { "display_name": "Bob" },
            "inline": { "path": "src/main.rs", "from": 17 }
        });
        let c: Comment = serde_json::from_value(raw).unwrap();
        let inline = c.inline.expect("inline present");
        assert_eq!(inline.from, Some(17));
        assert_eq!(inline.to, None);
    }

    #[test]
    fn test_comment_deserializes_global_comment_without_inline() {
        let raw = serde_json::json!({
            "id": 3,
            "content": { "raw": "LGTM" },
            "user": { "display_name": "Carol" }
        });
        let c: Comment = serde_json::from_value(raw).unwrap();
        assert!(c.inline.is_none());
    }

    #[test]
    fn test_format_comment_location_global_is_empty() {
        assert_eq!(format_comment_location(None), String::new());
    }

    #[test]
    fn test_format_comment_location_inline_new_side_line() {
        let inline = Inline {
            path: "src/main.rs".to_string(),
            to: Some(42),
            from: None,
        };
        assert_eq!(format_comment_location(Some(&inline)), "src/main.rs:42");
    }

    #[test]
    fn test_format_comment_location_inline_old_side_line() {
        let inline = Inline {
            path: "src/main.rs".to_string(),
            to: None,
            from: Some(17),
        };
        assert_eq!(format_comment_location(Some(&inline)), "src/main.rs:17");
    }

    #[test]
    fn test_format_comment_location_inline_file_level_no_line() {
        let inline = Inline {
            path: "README.md".to_string(),
            to: None,
            from: None,
        };
        assert_eq!(format_comment_location(Some(&inline)), "README.md");
    }

    #[test]
    fn test_format_comment_location_prefers_to_when_both_set() {
        // Bitbucket's docs say only one of `to`/`from` should be set for a
        // line-anchored inline comment, but if both ever come back, this pins
        // the deterministic "prefer `to` (destination revision)" tie-break.
        let inline = Inline {
            path: "src/main.rs".to_string(),
            to: Some(10),
            from: Some(20),
        };
        assert_eq!(format_comment_location(Some(&inline)), "src/main.rs:10");
    }

    #[test]
    fn test_format_comment_location_treats_zero_as_no_line() {
        // Bitbucket lines are 1-indexed and we reject `--line 0` on the way
        // out, so a 0 in the response is a sentinel or a serialisation glitch
        // rather than an anchor. Degrade to file-level rather than rendering
        // "src/main.rs:0".
        let inline_to_zero = Inline {
            path: "src/main.rs".to_string(),
            to: Some(0),
            from: None,
        };
        assert_eq!(
            format_comment_location(Some(&inline_to_zero)),
            "src/main.rs"
        );

        let inline_from_zero = Inline {
            path: "src/main.rs".to_string(),
            to: None,
            from: Some(0),
        };
        assert_eq!(
            format_comment_location(Some(&inline_from_zero)),
            "src/main.rs"
        );

        // A real `from` still wins when `to` is 0.
        let inline_to_zero_from_real = Inline {
            path: "src/main.rs".to_string(),
            to: Some(0),
            from: Some(17),
        };
        assert_eq!(
            format_comment_location(Some(&inline_to_zero_from_real)),
            "src/main.rs:17"
        );
    }

    #[test]
    fn test_inline_deserialises_when_path_is_missing() {
        // Defensive: if Bitbucket ever returns a stripped `inline` object
        // without `path`, we should still parse the comment and degrade
        // gracefully rather than fail the entire `list_pr_comments` call.
        let json = serde_json::json!({ "to": 42 });
        let inline: Inline = serde_json::from_value(json).expect("stripped inline must parse");
        assert_eq!(inline.path, "");
        assert_eq!(inline.to, Some(42));
        // format renders as ":42" — ugly but survivable, and only surfaces if
        // Bitbucket genuinely omits `path`, which is not documented behaviour.
        assert_eq!(format_comment_location(Some(&inline)), ":42");
    }
}
