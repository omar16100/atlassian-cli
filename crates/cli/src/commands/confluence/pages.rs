use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use super::utils::ConfluenceContext;

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

    #[derive(Deserialize)]
    struct Page {
        id: String,
        title: String,
        #[serde(rename = "type")]
        page_type: String,
        status: String,
    }

    let mut query_params = Vec::new();

    if let Some(l) = limit {
        query_params.push(format!("limit={}", l));
    }

    if let Some(sk) = space_key {
        query_params.push(format!("space-key={}", sk));
    }

    let query_string = if query_params.is_empty() {
        String::new()
    } else {
        format!("?{}", query_params.join("&"))
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
            title: p.title.as_str(),
            page_type: p.page_type.as_str(),
            status: p.status.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Get page details
pub async fn get_page(ctx: &ConfluenceContext<'_>, page_id: &str) -> Result<()> {
    let page: Value = ctx
        .client
        .get(&format!(
            "/wiki/api/v2/pages/{}?body-format=storage",
            page_id
        ))
        .await
        .with_context(|| format!("Failed to get page {}", page_id))?;

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
    println!("✅ Created page: {} (ID: {})", response.title, response.id);
    Ok(())
}

// Update page
pub async fn update_page(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    title: Option<&str>,
    body_file: Option<&PathBuf>,
) -> Result<()> {
    // Get current page first to get version
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

    let mut payload = json!({
        "id": page_id,
        "status": "current",
        "version": {
            "number": current_version + 1
        }
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

    tracing::info!(%page_id, "Page updated successfully");
    println!("✅ Updated page: {}", page_id);
    Ok(())
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
    println!("✅ Deleted page: {}", page_id);
    Ok(())
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
    println!("✅ Added label '{}' to page {}", label, page_id);
    Ok(())
}

// Remove page label
pub async fn remove_page_label(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    label: &str,
) -> Result<()> {
    let _: Value = ctx
        .client
        .delete(&format!(
            "/wiki/rest/api/content/{}/label?name={}",
            page_id, label
        ))
        .await
        .with_context(|| format!("Failed to remove label from page {}", page_id))?;

    tracing::info!(%page_id, %label, "Label removed successfully");
    println!("✅ Removed label '{}' from page {}", label, page_id);
    Ok(())
}

// List page comments
pub async fn list_page_comments(ctx: &ConfluenceContext<'_>, page_id: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct CommentsResponse {
        results: Vec<Comment>,
    }

    #[derive(Deserialize)]
    struct Comment {
        id: String,
        title: String,
        #[serde(rename = "createdAt")]
        created_at: String,
    }

    let response: CommentsResponse = ctx
        .client
        .get(&format!("/wiki/api/v2/pages/{}/footer-comments", page_id))
        .await
        .with_context(|| format!("Failed to list comments for page {}", page_id))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        title: &'a str,
        created_at: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .results
        .iter()
        .map(|c| Row {
            id: c.id.as_str(),
            title: c.title.as_str(),
            created_at: c.created_at.as_str(),
        })
        .collect();

    ctx.renderer.render(&rows)
}

// Add page comment
pub async fn add_page_comment(
    ctx: &ConfluenceContext<'_>,
    page_id: &str,
    comment: &str,
) -> Result<()> {
    let payload = json!({
        "pageId": page_id,
        "status": "current",
        "body": {
            "representation": "storage",
            "value": format!("<p>{}</p>", comment)
        }
    });

    #[derive(Deserialize)]
    struct CreateResponse {
        id: String,
    }

    let response: CreateResponse = ctx
        .client
        .post("/wiki/api/v2/footer-comments", &payload)
        .await
        .with_context(|| format!("Failed to add comment to page {}", page_id))?;

    tracing::info!(page_id = %page_id, comment_id = %response.id, "Comment added successfully");
    println!("✅ Added comment to page {} (ID: {})", page_id, response.id);
    Ok(())
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
    let _: Value = ctx
        .client
        .delete(&format!(
            "/wiki/rest/api/content/{}/restriction?operation={}&{}.identifier={}",
            page_id, operation, subject_type, subject_id
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

    let mut query_params = Vec::new();

    if let Some(l) = limit {
        query_params.push(format!("limit={}", l));
    }

    if let Some(sid) = space_id {
        query_params.push(format!("space-id={}", sid));
    }

    let query_string = if query_params.is_empty() {
        String::new()
    } else {
        format!("?{}", query_params.join("&"))
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
pub async fn get_blogpost(ctx: &ConfluenceContext<'_>, blogpost_id: &str) -> Result<()> {
    let blogpost: Value = ctx
        .client
        .get(&format!(
            "/wiki/api/v2/blogposts/{}?body-format=storage",
            blogpost_id
        ))
        .await
        .with_context(|| format!("Failed to get blog post {}", blogpost_id))?;

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
) -> Result<()> {
    // Get current blog post first to get version
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

    let mut payload = json!({
        "id": blogpost_id,
        "status": "current",
        "version": {
            "number": current_version + 1
        }
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

    tracing::info!(%blogpost_id, "Blog post updated successfully");
    println!("✅ Updated blog post: {}", blogpost_id);
    Ok(())
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
    println!("✅ Deleted blog post: {}", blogpost_id);
    Ok(())
}
