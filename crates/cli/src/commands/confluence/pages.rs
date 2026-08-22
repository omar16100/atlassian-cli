use anyhow::{Context, Result};
use atlassian_cli_output::OutputFormat;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use super::utils::ConfluenceContext;
use crate::commands::common::{render_success, MutationResult};
use crate::query::UrlParamsBuilder;

// List pages
pub async fn list_pages(
    ctx: &ConfluenceContext<'_>,
    space_key: Option<&str>,
    limit: Option<usize>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct PagesResponse {
        results: Vec<Page>,
    }

    // Fields beyond `id` are optional: the Confluence v2 pages list may omit `type`
    // entirely and can return null/absent `title`/`status` for some content, which
    // would otherwise abort the whole parse with "error decoding response body".
    #[derive(Deserialize)]
    struct Page {
        id: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(rename = "type", default)]
        page_type: Option<String>,
        #[serde(default)]
        status: Option<String>,
    }

    let query_string = {
        let mut builder = UrlParamsBuilder::new();

        if let Some(l) = limit {
            builder = builder.add("limit", &l.to_string());
        }

        builder = builder.add_optional("space-key", space_key);

        let params = builder.finish();
        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params)
        }
    };

    let response: PagesResponse = ctx
        .client
        .get(&format!("/wiki/api/v2/pages{}", query_string))
        .await
        .context("Failed to list pages")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        title: &'a str,
        page_type: &'a str,
        status: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .results
        .iter()
        .map(|p| Row {
            id: p.id.as_str(),
            title: p.title.as_deref().unwrap_or(""),
            page_type: p.page_type.as_deref().unwrap_or(""),
            status: p.status.as_deref().unwrap_or(""),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Get page details
pub async fn get_page(ctx: &ConfluenceContext<'_>, page_id: &str, body_only: bool) -> Result<()> {
    let page: Value = ctx
        .client
        .get(&format!(
            "/wiki/api/v2/pages/{}?body-format=storage",
            page_id
        ))
        .await
        .with_context(|| format!("Failed to get page {}", page_id))?;

    let format = ctx.renderer.format();

    if body_only {
        let body_html = page
            .get("body")
            .and_then(|b| b.get("storage"))
            .and_then(|s| s.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if format == OutputFormat::Markdown {
            let md = htmd::convert(body_html).context("Failed to convert HTML to markdown")?;
            println!("{}", md);
        } else {
            println!("{}", body_html);
        }
        return Ok(());
    }

    if format == OutputFormat::Markdown {
        let title = page
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("Untitled");
        let id = page.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let status = page.get("status").and_then(|s| s.as_str()).unwrap_or("");
        let body_html = page
            .get("body")
            .and_then(|b| b.get("storage"))
            .and_then(|s| s.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let body_md = htmd::convert(body_html).context("Failed to convert HTML to markdown")?;

        let md = format!(
            "# {title}\n\n\
             | Field | Value |\n\
             | --- | --- |\n\
             | ID | {id} |\n\
             | Status | {status} |\n\n\
             {body_md}"
        );
        return ctx.renderer.render_raw(&md);
    }

    println!("{}", serde_json::to_string_pretty(&page)?);
    Ok(())
}

// Create page
pub async fn create_page(
    ctx: &ConfluenceContext<'_>,
    space_id: &str,
    title: &str,
    body_file: Option<&PathBuf>,
    parent_id: Option<&str>,
) -> Result<()> {
    let body_content = if let Some(file) = body_file {
        fs::read_to_string(file)
            .with_context(|| format!("Failed to read body file: {}", file.display()))?
    } else {
        "<p>Page content</p>".to_string()
    };

    let mut payload = json!({
        "spaceId": space_id,
        "status": "current",
        "title": title,
        "body": {
            "representation": "storage",
            "value": body_content
        }
    });

    if let Some(pid) = parent_id {
        payload["parentId"] = json!(pid);
    }

    #[derive(Deserialize)]
    struct CreateResponse {
        id: String,
        title: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/wiki/api/v2/pages", &payload)
        .await
        .context("Failed to create page")?;

    tracing::info!(id = %response.id, title = %response.title, "Page created successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Created page: {} (ID: {})", response.title, response.id),
        &MutationResult::with_id(format!("Created page: {}", response.title), &response.id),
    )
}

// Update page
pub async fn update_page(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    title: Option<&str>,
    body_file: Option<&PathBuf>,
    target_status: Option<&str>,
    message: Option<&str>,
) -> Result<()> {
    // Get current page first to get version and status
    let current: Value = ctx
        .client
        .get(&format!("/wiki/api/v2/pages/{}", page_id))
        .await
        .with_context(|| format!("Failed to get page {}", page_id))?;

    let current_version = current
        .get("version")
        .and_then(|v| v.get("number"))
        .and_then(|n| n.as_i64())
        .unwrap_or(1);

    let current_status = current
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("current");

    // Determine target status (preserve current if not specified)
    let target_status = target_status.unwrap_or(current_status);

    // Calculate new version based on status transition
    let new_version = match (current_status, target_status) {
        ("draft", "current") => {
            tracing::info!(%page_id, "Publishing draft page with version 1");
            1 // First publish - MUST be 1
        }
        ("draft", "draft") => {
            tracing::debug!(%page_id, version = %current_version, "Updating draft, keeping version");
            current_version // Draft edit - no version bump needed
        }
        ("current", "current") => {
            tracing::debug!(%page_id, version = %(current_version + 1), "Updating published page, incrementing version");
            current_version + 1 // Normal update
        }
        ("current", "draft") => {
            anyhow::bail!(
                "Cannot change published page back to draft status. Page {} is already published.",
                page_id
            );
        }
        _ => current_version + 1,
    };

    let mut version_obj = json!({ "number": new_version });
    if let Some(msg) = message {
        version_obj["message"] = json!(msg);
    }

    let mut payload = json!({
        "id": page_id,
        "status": target_status,
        "version": version_obj
    });

    if let Some(t) = title {
        payload["title"] = json!(t);
    } else {
        payload["title"] = current.get("title").cloned().unwrap_or(json!("Untitled"));
    }

    if let Some(file) = body_file {
        let body_content = fs::read_to_string(file)
            .with_context(|| format!("Failed to read body file: {}", file.display()))?;
        payload["body"] = json!({
            "representation": "storage",
            "value": body_content
        });
    }

    let _: Value = ctx
        .client
        .put(&format!("/wiki/api/v2/pages/{}", page_id), &payload)
        .await
        .with_context(|| format!("Failed to update page {}", page_id))?;

    tracing::info!(%page_id, status = %target_status, version = %new_version, "Page updated successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Updated page: {page_id} (v{new_version}, status: {target_status})"),
        &MutationResult::with_id(format!("Updated page: {page_id}"), page_id),
    )
}

/// Publish a draft page for the first time
pub async fn publish_page(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    title: Option<&str>,
    body_file: &PathBuf,
    message: Option<&str>,
) -> Result<()> {
    // Get current page to verify it's a draft
    let current: Value = ctx
        .client
        .get(&format!("/wiki/api/v2/pages/{}", page_id))
        .await
        .with_context(|| format!("Failed to get page {}", page_id))?;

    let current_status = current
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("current");

    if current_status != "draft" {
        anyhow::bail!(
            "Page {} is already published (status: '{}').\nUse 'confluence page update' to update published pages.",
            page_id,
            current_status
        );
    }

    let body_content = fs::read_to_string(body_file)
        .with_context(|| format!("Failed to read body file: {}", body_file.display()))?;

    let page_title = title
        .map(|t| t.to_string())
        .or_else(|| {
            current
                .get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "Untitled".to_string());

    let mut version_obj = json!({ "number": 1 });
    if let Some(msg) = message {
        version_obj["message"] = json!(msg);
    } else {
        version_obj["message"] = json!("Published via CLI");
    }

    let payload = json!({
        "id": page_id,
        "status": "current",
        "title": page_title,
        "version": version_obj,
        "body": {
            "representation": "storage",
            "value": body_content
        }
    });

    tracing::info!(%page_id, title = %page_title, "Publishing draft page");

    let _: Value = ctx
        .client
        .put(&format!("/wiki/api/v2/pages/{}", page_id), &payload)
        .await
        .with_context(|| format!("Failed to publish page {}", page_id))?;

    tracing::info!(%page_id, "Draft page published successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Published page: {} (ID: {})", page_title, page_id),
        &MutationResult::with_id(format!("Published page: {}", page_title), page_id),
    )
}

// Delete page
pub async fn delete_page(ctx: &ConfluenceContext<'_>, page_id: &str, force: bool) -> Result<()> {
    if !force {
        println!(
            "⚠️  This will permanently delete page {}. Use --force to confirm.",
            page_id
        );
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!("/wiki/api/v2/pages/{}", page_id))
        .await
        .with_context(|| format!("Failed to delete page {}", page_id))?;

    tracing::info!(%page_id, "Page deleted successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Deleted page: {page_id}"),
        &MutationResult::with_id(format!("Deleted page: {page_id}"), page_id),
    )
}

// List page versions
pub async fn list_page_versions(ctx: &ConfluenceContext<'_>, page_id: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct VersionsResponse {
        results: Vec<PageVersion>,
    }

    #[derive(Deserialize)]
    struct PageVersion {
        number: i64,
        message: Option<String>,
        #[serde(rename = "createdAt")]
        created_at: String,
    }

    let response: VersionsResponse = ctx
        .client
        .get(&format!("/wiki/api/v2/pages/{}/versions", page_id))
        .await
        .with_context(|| format!("Failed to list versions for page {}", page_id))?;

    #[derive(Serialize)]
    struct Row<'a> {
        number: i64,
        message: &'a str,
        created_at: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .results
        .iter()
        .map(|v| Row {
            number: v.number,
            message: v.message.as_deref().unwrap_or(""),
            created_at: v.created_at.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Add page label
pub async fn add_page_label(ctx: &ConfluenceContext<'_>, page_id: &str, label: &str) -> Result<()> {
    let payload = json!([{
        "prefix": "global",
        "name": label
    }]);

    let _: Value = ctx
        .client
        .post(
            &format!("/wiki/rest/api/content/{}/label", page_id),
            &payload,
        )
        .await
        .with_context(|| format!("Failed to add label to page {}", page_id))?;

    tracing::info!(%page_id, %label, "Label added successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Added label '{label}' to page {page_id}"),
        &MutationResult::with_id(format!("Added label '{label}' to page {page_id}"), page_id),
    )
}

// Remove page label
pub async fn remove_page_label(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    label: &str,
) -> Result<()> {
    let query_string = UrlParamsBuilder::new().add("name", label).finish();

    let _: Value = ctx
        .client
        .delete(&format!(
            "/wiki/rest/api/content/{}/label?{}",
            page_id, query_string
        ))
        .await
        .with_context(|| format!("Failed to remove label from page {}", page_id))?;

    tracing::info!(%page_id, %label, "Label removed successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Removed label '{label}' from page {page_id}"),
        &MutationResult::with_id(
            format!("Removed label '{label}' from page {page_id}"),
            page_id,
        ),
    )
}

/// A footer comment as returned by `GET /wiki/api/v2/pages/{id}/footer-comments`.
///
/// Everything beyond `id` is optional. Per the Confluence Cloud v2 schema
/// (`FooterCommentModel`) a comment has **no top-level `createdAt`** — the
/// timestamp lives at `version.createdAt`. Deserialising `createdAt` as a
/// required top-level field made this command fail with
/// "missing field `createdAt`" on any page that had at least one comment.
/// `body` is only returned when `body-format` is requested.
#[derive(Deserialize)]
struct Comment {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    version: Option<CommentVersion>,
    #[serde(default)]
    body: Option<CommentBody>,
}

#[derive(Deserialize)]
struct CommentVersion {
    #[serde(rename = "createdAt", default)]
    created_at: Option<String>,
}

#[derive(Deserialize)]
struct CommentBody {
    #[serde(default)]
    storage: Option<CommentBodyValue>,
}

#[derive(Deserialize)]
struct CommentBodyValue {
    #[serde(default)]
    value: String,
}

#[derive(Deserialize)]
struct CommentsResponse {
    #[serde(default)]
    results: Vec<Comment>,
    /// Confluence v2 paginates with an opaque cursor in `_links.next`. Ignoring
    /// it silently truncated every listing at the default page size, so a page
    /// with 84 comments reported 25.
    #[serde(rename = "_links", default)]
    links: Option<CommentLinks>,
}

#[derive(Deserialize)]
struct CommentLinks {
    #[serde(default)]
    next: Option<String>,
}

/// Where a comment lives on the page.
///
/// Confluence keeps these in separate collections with separate endpoints, and
/// only footer comments were ever fetched, so pages that use inline comments
/// (the highlight-and-comment kind) looked empty from the CLI.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CommentKind {
    Footer,
    Inline,
}

impl CommentKind {
    fn as_str(self) -> &'static str {
        match self {
            CommentKind::Footer => "footer",
            CommentKind::Inline => "inline",
        }
    }

    /// The v2 collection this kind lives in, used for both listing replies and
    /// creating them.
    fn collection(self) -> &'static str {
        match self {
            CommentKind::Footer => "footer-comments",
            CommentKind::Inline => "inline-comments",
        }
    }
}

/// Follow `_links.next` until the collection is exhausted.
///
/// The cap is a safety net against a server that keeps handing back a cursor;
/// at the v2 default of 25 per page it allows 5,000 comments, far past anything
/// a real page carries.
async fn fetch_all_comments(ctx: &ConfluenceContext<'_>, first_path: &str) -> Result<Vec<Comment>> {
    const MAX_PAGES: usize = 200;

    let mut all = Vec::new();
    let mut path = first_path.to_string();

    for page in 0..MAX_PAGES {
        let response: CommentsResponse = ctx
            .client
            .get(&path)
            .await
            .with_context(|| format!("Failed to fetch {path}"))?;
        all.extend(response.results);

        match response.links.and_then(|l| l.next) {
            Some(next) if !next.is_empty() => path = next,
            _ => return Ok(all),
        }

        if page + 1 == MAX_PAGES {
            tracing::warn!(
                fetched = all.len(),
                "Stopped following comment pagination at the safety cap"
            );
        }
    }
    Ok(all)
}

/// The comment's creation timestamp, which the v2 API nests under `version`.
fn comment_created_at(comment: &Comment) -> &str {
    comment
        .version
        .as_ref()
        .and_then(|v| v.created_at.as_deref())
        .unwrap_or("")
}

/// Return the raw storage-format (HTML) body of a comment, if one was returned.
fn comment_storage_html(comment: &Comment) -> Option<&str> {
    comment
        .body
        .as_ref()
        .and_then(|b| b.storage.as_ref())
        .map(|s| s.value.as_str())
        .filter(|v| !v.is_empty())
}

/// Convert a comment's storage-format body to markdown for display. Falls back
/// to an empty string when no body was returned or conversion fails.
fn comment_body_markdown(comment: &Comment) -> String {
    match comment_storage_html(comment) {
        Some(html) => htmd::convert(html).unwrap_or_default().trim().to_string(),
        None => String::new(),
    }
}

/// Collapse a body to a single-line preview.
///
/// The table renderer does not wrap, `render_csv` does not quote or escape, and
/// `render_markdown_table` does not escape newlines, so an unbounded multi-line
/// body would corrupt those outputs. Mirrors `jira issue comments`, which shows
/// a short preview by default and the full body behind `--full`.
fn comment_body_preview(body: &str) -> String {
    let flattened = body.split_whitespace().collect::<Vec<_>>().join(" ");
    flattened.chars().take(COMMENT_PREVIEW_CHARS).collect()
}

/// Preview length, matching `jira issue comments`.
const COMMENT_PREVIEW_CHARS: usize = 50;

/// A comment plus where it came from, so the two collections can be merged into
/// one listing without losing which is which.
struct LocatedComment {
    comment: Comment,
    kind: CommentKind,
    /// The comment this one replies to. `None` for a thread root.
    parent: Option<String>,
}

/// List page comments.
///
/// Fetches both collections, because Confluence keeps footer and inline
/// comments apart and only footer comments were ever read: a page using inline
/// comments looked empty from the CLI even with dozens of them in the browser.
///
/// `replies` walks each thread. That is one request per root comment, so it is
/// opt-in rather than the default.
pub async fn list_page_comments(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    full: bool,
    replies: bool,
) -> Result<()> {
    let mut located: Vec<LocatedComment> = Vec::new();

    for kind in [CommentKind::Footer, CommentKind::Inline] {
        // body-format=storage so callers get the text, not just metadata.
        let path = format!(
            "/wiki/api/v2/pages/{}/{}?body-format=storage",
            page_id,
            kind.collection()
        );
        let roots = fetch_all_comments(ctx, &path).await.with_context(|| {
            format!("Failed to list {} for page {}", kind.collection(), page_id)
        })?;

        for root in roots {
            let root_id = root.id.clone();
            located.push(LocatedComment {
                comment: root,
                kind,
                parent: None,
            });

            if replies {
                let child_path = format!(
                    "/wiki/api/v2/{}/{}/children?body-format=storage",
                    kind.collection(),
                    root_id
                );
                let children = fetch_all_comments(ctx, &child_path)
                    .await
                    .with_context(|| format!("Failed to list replies to comment {root_id}"))?;
                for child in children {
                    located.push(LocatedComment {
                        comment: child,
                        kind,
                        parent: Some(root_id.clone()),
                    });
                }
            }
        }
    }

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        title: &'a str,
        created_at: &'a str,
        /// footer or inline. Also tells you which value to pass to
        /// `add-comment --kind` when replying.
        kind: &'static str,
        /// The comment this replies to, absent for a thread root.
        #[serde(skip_serializing_if = "Option::is_none")]
        parent: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body_preview: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    }

    let rows: Vec<Row<'_>> = located
        .iter()
        .map(|l| {
            let text = comment_body_markdown(&l.comment);
            let (body_preview, body) = if full {
                (None, Some(text))
            } else {
                (Some(comment_body_preview(&text)), None)
            };
            Row {
                id: l.comment.id.as_str(),
                title: l.comment.title.as_str(),
                created_at: comment_created_at(&l.comment),
                kind: l.kind.as_str(),
                parent: l.parent.as_deref(),
                body_preview,
                body,
            }
        })
        .collect();

    ctx.renderer
        .render_list_or_empty(&rows, "No comments found")
}

/// Escape text for a storage-format (XHTML) body.
///
/// The comment was previously interpolated raw into `<p>{}</p>`, so a comment
/// containing `<`, `>` or `&` produced invalid storage XHTML and Confluence
/// either rejected it or mangled it.
fn escape_storage_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Build the body for creating a comment or a reply.
///
/// A reply carries `parentCommentId` and is POSTed to the collection its parent
/// lives in: Confluence will not accept a footer reply to an inline thread.
fn comment_payload(page_id: &str, comment: &str, parent: Option<&str>) -> serde_json::Value {
    let mut payload = json!({
        "pageId": page_id,
        "status": "current",
        "body": {
            "representation": "storage",
            "value": format!("<p>{}</p>", escape_storage_text(comment))
        }
    });
    if let Some(parent) = parent {
        payload["parentCommentId"] = json!(parent);
    }
    payload
}

/// Add a comment to a page, or reply to an existing thread.
///
/// `kind` selects the collection. It matters for replies: an inline thread only
/// accepts replies posted to `inline-comments`. The `kind` column on
/// `page comments` tells you which to pass.
pub async fn add_page_comment(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    comment: &str,
    parent: Option<&str>,
    kind: CommentKind,
) -> Result<()> {
    let payload = comment_payload(page_id, comment, parent);

    #[derive(Deserialize)]
    struct CreateResponse {
        id: String,
    }

    let response: CreateResponse = ctx
        .client
        .post(&format!("/wiki/api/v2/{}", kind.collection()), &payload)
        .await
        .with_context(|| match parent {
            Some(parent) => format!("Failed to reply to comment {parent}"),
            None => format!("Failed to add comment to page {}", page_id),
        })?;

    tracing::info!(
        page_id = %page_id,
        comment_id = %response.id,
        kind = kind.as_str(),
        is_reply = parent.is_some(),
        "Comment added successfully"
    );
    let message = match parent {
        Some(parent) => format!("Replied to comment {parent} on page {page_id}"),
        None => format!("Added comment to page {page_id}"),
    };
    render_success(
        ctx.renderer,
        &format!("✅ {message} (ID: {})", response.id),
        &MutationResult::with_id(message, &response.id),
    )
}

// Get page restrictions
pub async fn get_page_restrictions(ctx: &ConfluenceContext<'_>, page_id: &str) -> Result<()> {
    let restrictions: Value = ctx
        .client
        .get(&format!("/wiki/rest/api/content/{}/restriction", page_id))
        .await
        .with_context(|| format!("Failed to get restrictions for page {}", page_id))?;

    println!("{}", serde_json::to_string_pretty(&restrictions)?);
    Ok(())
}

// Add page restriction
pub async fn add_page_restriction(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    operation: &str,
    subject_type: &str,
    subject_id: &str,
) -> Result<()> {
    let payload = json!({
        "operation": operation,
        "restrictions": {
            subject_type: [{
                "type": subject_type,
                "identifier": subject_id
            }]
        }
    });

    let _: Value = ctx
        .client
        .post(
            &format!("/wiki/rest/api/content/{}/restriction", page_id),
            &payload,
        )
        .await
        .with_context(|| format!("Failed to add restriction to page {}", page_id))?;

    tracing::info!(%page_id, %operation, %subject_id, "Restriction added successfully");
    println!(
        "✅ Added {} restriction for {} to page {}",
        operation, subject_id, page_id
    );
    Ok(())
}

// Remove page restriction
pub async fn remove_page_restriction(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    operation: &str,
    subject_type: &str,
    subject_id: &str,
) -> Result<()> {
    let query_string = UrlParamsBuilder::new()
        .add("operation", operation)
        .add(&format!("{}.identifier", subject_type), subject_id)
        .finish();

    let _: Value = ctx
        .client
        .delete(&format!(
            "/wiki/rest/api/content/{}/restriction?{}",
            page_id, query_string
        ))
        .await
        .with_context(|| format!("Failed to remove restriction from page {}", page_id))?;

    tracing::info!(%page_id, %operation, %subject_id, "Restriction removed successfully");
    println!(
        "✅ Removed {} restriction for {} from page {}",
        operation, subject_id, page_id
    );
    Ok(())
}

// List blog posts
pub async fn list_blogposts(
    ctx: &ConfluenceContext<'_>,
    space_id: Option<&str>,
    limit: Option<usize>,
) -> Result<()> {
    #[derive(Deserialize)]
    struct BlogpostsResponse {
        results: Vec<Blogpost>,
    }

    #[derive(Deserialize)]
    struct Blogpost {
        id: String,
        title: String,
        status: String,
    }

    let query_string = {
        let mut builder = UrlParamsBuilder::new();

        if let Some(l) = limit {
            builder = builder.add("limit", &l.to_string());
        }

        builder = builder.add_optional("space-id", space_id);

        let params = builder.finish();
        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params)
        }
    };

    let response: BlogpostsResponse = ctx
        .client
        .get(&format!("/wiki/api/v2/blogposts{}", query_string))
        .await
        .context("Failed to list blog posts")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        title: &'a str,
        status: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .results
        .iter()
        .map(|b| Row {
            id: b.id.as_str(),
            title: b.title.as_str(),
            status: b.status.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Get blog post details
pub async fn get_blogpost(
    ctx: &ConfluenceContext<'_>,
    blogpost_id: &str,
    body_only: bool,
) -> Result<()> {
    let blogpost: Value = ctx
        .client
        .get(&format!(
            "/wiki/api/v2/blogposts/{}?body-format=storage",
            blogpost_id
        ))
        .await
        .with_context(|| format!("Failed to get blog post {}", blogpost_id))?;

    let format = ctx.renderer.format();

    if body_only {
        let body_html = blogpost
            .get("body")
            .and_then(|b| b.get("storage"))
            .and_then(|s| s.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if format == OutputFormat::Markdown {
            let md = htmd::convert(body_html).context("Failed to convert HTML to markdown")?;
            println!("{}", md);
        } else {
            println!("{}", body_html);
        }
        return Ok(());
    }

    if format == OutputFormat::Markdown {
        let title = blogpost
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("Untitled");
        let id = blogpost.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let status = blogpost
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        let body_html = blogpost
            .get("body")
            .and_then(|b| b.get("storage"))
            .and_then(|s| s.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let body_md = htmd::convert(body_html).context("Failed to convert HTML to markdown")?;

        let md = format!(
            "# {title}\n\n\
             | Field | Value |\n\
             | --- | --- |\n\
             | ID | {id} |\n\
             | Status | {status} |\n\n\
             {body_md}"
        );
        return ctx.renderer.render_raw(&md);
    }

    println!("{}", serde_json::to_string_pretty(&blogpost)?);
    Ok(())
}

// Create blog post
pub async fn create_blog(
    ctx: &ConfluenceContext<'_>,
    space_id: &str,
    title: &str,
    body_file: Option<&PathBuf>,
) -> Result<()> {
    let body_content = if let Some(file) = body_file {
        fs::read_to_string(file)
            .with_context(|| format!("Failed to read body file: {}", file.display()))?
    } else {
        "<p>Blog post content</p>".to_string()
    };

    let payload = json!({
        "spaceId": space_id,
        "status": "current",
        "title": title,
        "type": "blogpost",
        "body": {
            "representation": "storage",
            "value": body_content
        }
    });

    #[derive(Deserialize)]
    struct CreateResponse {
        id: String,
        title: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/wiki/api/v2/blogposts", &payload)
        .await
        .context("Failed to create blog post")?;

    tracing::info!(id = %response.id, title = %response.title, "Blog post created successfully");
    println!(
        "✅ Created blog post: {} (ID: {})",
        response.title, response.id
    );
    Ok(())
}

// Update blog post
pub async fn update_blogpost(
    ctx: &ConfluenceContext<'_>,
    blogpost_id: &str,
    title: Option<&str>,
    body_file: Option<&PathBuf>,
    target_status: Option<&str>,
    message: Option<&str>,
) -> Result<()> {
    // Get current blog post first to get version and status
    let current: Value = ctx
        .client
        .get(&format!("/wiki/api/v2/blogposts/{}", blogpost_id))
        .await
        .with_context(|| format!("Failed to get blog post {}", blogpost_id))?;

    let current_version = current
        .get("version")
        .and_then(|v| v.get("number"))
        .and_then(|n| n.as_i64())
        .unwrap_or(1);

    let current_status = current
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("current");

    // Determine target status (preserve current if not specified)
    let target_status = target_status.unwrap_or(current_status);

    // Calculate new version based on status transition
    let new_version = match (current_status, target_status) {
        ("draft", "current") => {
            tracing::info!(%blogpost_id, "Publishing draft blog post with version 1");
            1 // First publish - MUST be 1
        }
        ("draft", "draft") => {
            tracing::debug!(%blogpost_id, version = %current_version, "Updating draft, keeping version");
            current_version // Draft edit - no version bump needed
        }
        ("current", "current") => {
            tracing::debug!(%blogpost_id, version = %(current_version + 1), "Updating published blog post, incrementing version");
            current_version + 1 // Normal update
        }
        ("current", "draft") => {
            anyhow::bail!(
                "Cannot change published blog post back to draft status. Blog post {} is already published.",
                blogpost_id
            );
        }
        _ => current_version + 1,
    };

    let mut version_obj = json!({ "number": new_version });
    if let Some(msg) = message {
        version_obj["message"] = json!(msg);
    }

    let mut payload = json!({
        "id": blogpost_id,
        "status": target_status,
        "version": version_obj
    });

    if let Some(t) = title {
        payload["title"] = json!(t);
    } else {
        payload["title"] = current.get("title").cloned().unwrap_or(json!("Untitled"));
    }

    if let Some(file) = body_file {
        let body_content = fs::read_to_string(file)
            .with_context(|| format!("Failed to read body file: {}", file.display()))?;
        payload["body"] = json!({
            "representation": "storage",
            "value": body_content
        });
    }

    let _: Value = ctx
        .client
        .put(&format!("/wiki/api/v2/blogposts/{}", blogpost_id), &payload)
        .await
        .with_context(|| format!("Failed to update blog post {}", blogpost_id))?;

    tracing::info!(%blogpost_id, status = %target_status, version = %new_version, "Blog post updated successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Updated blog post: {blogpost_id} (v{new_version}, status: {target_status})"),
        &MutationResult::with_id(format!("Updated blog post: {blogpost_id}"), blogpost_id),
    )
}

/// Publish a draft blog post for the first time
pub async fn publish_blogpost(
    ctx: &ConfluenceContext<'_>,
    blogpost_id: &str,
    title: Option<&str>,
    body_file: &PathBuf,
    message: Option<&str>,
) -> Result<()> {
    // Get current blog post to verify it's a draft
    let current: Value = ctx
        .client
        .get(&format!("/wiki/api/v2/blogposts/{}", blogpost_id))
        .await
        .with_context(|| format!("Failed to get blog post {}", blogpost_id))?;

    let current_status = current
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("current");

    if current_status != "draft" {
        anyhow::bail!(
            "Blog post {} is already published (status: '{}').\nUse 'confluence blog update' to update published blog posts.",
            blogpost_id,
            current_status
        );
    }

    let body_content = fs::read_to_string(body_file)
        .with_context(|| format!("Failed to read body file: {}", body_file.display()))?;

    let blogpost_title = title
        .map(|t| t.to_string())
        .or_else(|| {
            current
                .get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "Untitled".to_string());

    let mut version_obj = json!({ "number": 1 });
    if let Some(msg) = message {
        version_obj["message"] = json!(msg);
    } else {
        version_obj["message"] = json!("Published via CLI");
    }

    let payload = json!({
        "id": blogpost_id,
        "status": "current",
        "title": blogpost_title,
        "version": version_obj,
        "body": {
            "representation": "storage",
            "value": body_content
        }
    });

    tracing::info!(%blogpost_id, title = %blogpost_title, "Publishing draft blog post");

    let _: Value = ctx
        .client
        .put(&format!("/wiki/api/v2/blogposts/{}", blogpost_id), &payload)
        .await
        .with_context(|| format!("Failed to publish blog post {}", blogpost_id))?;

    tracing::info!(%blogpost_id, "Draft blog post published successfully");
    render_success(
        ctx.renderer,
        &format!(
            "✅ Published blog post: {} (ID: {})",
            blogpost_title, blogpost_id
        ),
        &MutationResult::with_id(
            format!("Published blog post: {}", blogpost_title),
            blogpost_id,
        ),
    )
}

// Delete blog post
pub async fn delete_blogpost(
    ctx: &ConfluenceContext<'_>,
    blogpost_id: &str,
    force: bool,
) -> Result<()> {
    if !force {
        println!(
            "⚠️  This will permanently delete blog post {}. Use --force to confirm.",
            blogpost_id
        );
        return Ok(());
    }

    let _: Value = ctx
        .client
        .delete(&format!("/wiki/api/v2/blogposts/{}", blogpost_id))
        .await
        .with_context(|| format!("Failed to delete blog post {}", blogpost_id))?;

    tracing::info!(%blogpost_id, "Blog post deleted successfully");
    render_success(
        ctx.renderer,
        &format!("✅ Deleted blog post: {blogpost_id}"),
        &MutationResult::with_id(format!("Deleted blog post: {blogpost_id}"), blogpost_id),
    )
}

#[cfg(test)]
mod comment_tests {
    use super::*;

    // Regression: per the v2 schema (FooterCommentModel) a comment has NO
    // top-level `createdAt` -- the timestamp is at `version.createdAt`. The old
    // code required a top-level `createdAt: String`, so `page comments` failed
    // with "missing field `createdAt`" on ANY page that had a comment.
    #[test]
    fn comment_reads_created_at_from_version() {
        let json = r#"{
            "id": "42",
            "title": "Re: X",
            "version": { "number": 1, "createdAt": "2026-01-02T03:04:05Z" }
        }"#;
        let c: Comment = serde_json::from_str(json).unwrap();
        assert_eq!(c.id, "42");
        assert_eq!(comment_created_at(&c), "2026-01-02T03:04:05Z");
    }

    // The real-world payload that used to abort the whole command.
    #[test]
    fn comment_without_top_level_created_at_still_parses() {
        let c: Comment = serde_json::from_str(r#"{"id":"42","title":"Re: X"}"#).unwrap();
        assert_eq!(c.id, "42");
        assert_eq!(comment_created_at(&c), "");
    }

    #[test]
    fn comment_tolerates_missing_title_and_body() {
        let c: Comment = serde_json::from_str(r#"{"id":"1"}"#).unwrap();
        assert_eq!(c.title, "");
        assert!(comment_storage_html(&c).is_none());
        assert_eq!(comment_body_markdown(&c), "");
    }

    // Body is only present when body-format is requested; when present we take the
    // storage HTML and convert it to markdown.
    #[test]
    fn comment_body_converts_storage_html_to_markdown() {
        let json = r#"{
            "id": "9",
            "body": { "storage": { "value": "<p>Hello <strong>world</strong></p>", "representation": "storage" } }
        }"#;
        let c: Comment = serde_json::from_str(json).unwrap();
        let md = comment_body_markdown(&c);
        assert!(md.contains("Hello"), "got {md:?}");
        assert!(md.contains("world"), "got {md:?}");
    }

    #[test]
    fn comment_empty_body_value_is_treated_as_absent() {
        let c: Comment =
            serde_json::from_str(r#"{"id":"3","body":{"storage":{"value":""}}}"#).unwrap();
        assert!(comment_storage_html(&c).is_none());
    }

    // A response mixing comments with and without version must fully parse
    // (previously a single comment failed the entire list).
    #[test]
    fn comments_response_with_mixed_fields_fully_parses() {
        let json = r#"{
            "results": [
                {"id":"1","title":"a","version":{"createdAt":"2026-01-01T00:00:00Z"}},
                {"id":"2","title":"b"},
                {"id":"3"}
            ]
        }"#;
        let resp: CommentsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.results.len(), 3);
        assert_eq!(comment_created_at(&resp.results[0]), "2026-01-01T00:00:00Z");
        assert_eq!(comment_created_at(&resp.results[1]), "");
    }

    #[test]
    fn comments_response_empty_results_parses() {
        let resp: CommentsResponse = serde_json::from_str(r#"{"results":[]}"#).unwrap();
        assert!(resp.results.is_empty());
    }

    // The table/CSV/markdown renderers do not wrap, quote or escape newlines, so
    // the default preview must be a single line and bounded.
    #[test]
    fn body_preview_is_single_line_and_bounded() {
        let body = "line one\nline two\n\nline three";
        let preview = comment_body_preview(body);
        assert!(!preview.contains('\n'), "preview must not contain newlines");
        assert_eq!(preview, "line one line two line three");

        let long = "x".repeat(200);
        assert_eq!(
            comment_body_preview(&long).chars().count(),
            COMMENT_PREVIEW_CHARS
        );
    }
}
