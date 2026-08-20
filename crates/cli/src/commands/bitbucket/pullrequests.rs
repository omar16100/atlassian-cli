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

/// Which side of the diff a line-anchored inline comment attaches to.
///
/// - `New` (default): the destination revision. Comments an added or unchanged line.
///   Serialised as Bitbucket's `inline.to` field.
/// - `Old`: the source revision. Comments a removed line.
///   Serialised as Bitbucket's `inline.from` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Side {
    #[default]
    #[value(name = "new")]
    New,
    #[value(name = "old")]
    Old,
}

/// Build the JSON body for a Bitbucket PR-comment POST.
///
/// Pure function so its shape is unit-testable without a mock server.
/// Matches the API contract from
/// <https://developer.atlassian.com/cloud/bitbucket/rest/api-group-pullrequests/>.
///
/// - `content`: comment text (Bitbucket renders as Markdown).
/// - `inline_path`: if `Some(_)`, comment is inline (attached to a file); if `None`, comment is global.
/// - `inline_line`: only meaningful when `inline_path.is_some()`. If `Some(n)`, the comment
///   is line-anchored; if `None`, it's a file-level inline comment.
/// - `side`: only meaningful when both `inline_path` and `inline_line` are `Some(_)`.
///   `New` → `inline.to = <n>`, `Old` → `inline.from = <n>`.
pub fn build_comment_payload(
    content: &str,
    inline_path: Option<&str>,
    inline_line: Option<u32>,
    side: Side,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "content": { "raw": content }
    });

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
        println!("No pull requests found");
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
        tracing::info!(pr_id, workspace, repo_slug, "No comments found");
        println!("No comments found");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

#[allow(clippy::too_many_arguments)]
pub async fn add_pr_comment(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    content: &str,
    inline_path: Option<&str>,
    inline_line: Option<u32>,
    side: Side,
) -> Result<()> {
    let payload = build_comment_payload(content, inline_path, inline_line, side);

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

    let emoji_message = if is_inline {
        format!("✅ Inline comment added to pull request #{pr_id}")
    } else {
        format!("✅ Comment added to pull request #{pr_id}")
    };
    let mutation_message = if is_inline {
        format!("Inline comment added to pull request #{pr_id}")
    } else {
        format!("Comment added to pull request #{pr_id}")
    };

    render_success(
        ctx.renderer,
        &emoji_message,
        &MutationResult::with_id(mutation_message, pr_id.to_string()),
    )
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

    #[derive(Serialize)]
    struct Row<'a> {
        name: &'a str,
        role: &'a str,
        status: &'a str,
        participated_on: &'a str,
    }

    let participants = pr.participants.unwrap_or_default();
    let rows: Vec<Row<'_>> = participants
        .iter()
        .filter(|p| show_all || p.role == "REVIEWER")
        .map(|p| Row {
            name: p.user.display_name.as_str(),
            role: p.role.as_str(),
            status: participant_status(p.approved, p.state.as_deref()),
            participated_on: p.participated_on.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(pr_id, workspace, repo_slug, show_all, "No reviewers found");
        println!("No reviewers found");
        return Ok(());
    }

    ctx.renderer.render(&rows)
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

    #[test]
    fn test_build_comment_payload_global() {
        let payload = build_comment_payload("Looks good!", None, None, Side::New);
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Looks good!" }
            })
        );
    }

    #[test]
    fn test_build_comment_payload_inline_new_side_line() {
        let payload = build_comment_payload(
            "Nit: rename this",
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
        let payload = build_comment_payload(
            "Why remove this?",
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
        let payload = build_comment_payload(
            "Whole-file comment",
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
        let new_side = build_comment_payload("Whole-file comment", Some("README.md"), None, Side::New);
        let old_side = build_comment_payload("Whole-file comment", Some("README.md"), None, Side::Old);
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
    fn test_add_pr_comment_signature_compiles() {
        // Sentinel test: the fact this module compiles at all means
        // `add_pr_comment`'s new signature (with Option<&str>, Option<u32>, Side)
        // is in place. The actual payload shape is covered by
        // `test_build_comment_payload_*` above and by the wire-level
        // integration tests in tests/bitbucket_integration.rs.
        //
        // If the signature regresses, mod.rs (Task 4) will fail to compile.
    }
}
