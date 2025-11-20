use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use super::utils::BitbucketContext;

#[derive(Deserialize)]
struct CommitList {
    values: Vec<Commit>,
}

#[derive(Deserialize)]
struct Commit {
    hash: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    author: Option<Author>,
    #[serde(default)]
    parents: Vec<Parent>,
}

#[derive(Deserialize)]
struct Author {
    #[serde(default)]
    raw: Option<String>,
    #[serde(default)]
    user: Option<User>,
}

#[derive(Deserialize)]
struct User {
    display_name: String,
}

#[derive(Deserialize)]
struct Parent {
    hash: String,
}

#[derive(Deserialize)]
struct DiffStat {
    #[serde(default)]
    values: Vec<FileDiff>,
}

#[derive(Deserialize)]
struct FileDiff {
    #[serde(default)]
    old: Option<FileInfo>,
    #[serde(default)]
    new: Option<FileInfo>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    lines_added: Option<i64>,
    #[serde(default)]
    lines_removed: Option<i64>,
}

#[derive(Deserialize)]
struct FileInfo {
    path: String,
}

#[derive(Deserialize)]
struct SourceFile {
    path: String,
    #[serde(rename = "type")]
    file_type: String,
    #[serde(default)]
    size: Option<i64>,
}

#[derive(Deserialize)]
struct SourceList {
    values: Vec<SourceFile>,
}

pub async fn list_commits(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    branch: Option<&str>,
    limit: usize,
) -> Result<()> {
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("pagelen", &limit.min(100).to_string());

    let path = if let Some(b) = branch {
        format!("/2.0/repositories/{workspace}/{repo_slug}/commits/{b}?{}", query.finish())
    } else {
        format!("/2.0/repositories/{workspace}/{repo_slug}/commits?{}", query.finish())
    };

    let response: CommitList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list commits for {workspace}/{repo_slug}"))?;

    #[derive(Serialize)]
    struct Row<'a> {
        hash: &'a str,
        author: &'a str,
        message: &'a str,
        date: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|commit| Row {
            hash: &commit.hash[..7.min(commit.hash.len())],
            author: commit
                .author
                .as_ref()
                .and_then(|a| a.user.as_ref().map(|u| u.display_name.as_str()))
                .or_else(|| {
                    commit
                        .author
                        .as_ref()
                        .and_then(|a| a.raw.as_deref())
                })
                .unwrap_or(""),
            message: commit
                .message
                .as_deref()
                .and_then(|m| m.lines().next())
                .unwrap_or(""),
            date: commit.date.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(workspace, repo_slug, "No commits found");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn get_commit(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    commit_hash: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/commit/{commit_hash}");
    let commit: Commit = ctx
        .client
        .get(&path)
        .await
        .with_context(|| {
            format!("Failed to fetch commit {commit_hash} for {workspace}/{repo_slug}")
        })?;

    #[derive(Serialize)]
    struct View<'a> {
        hash: &'a str,
        author: &'a str,
        date: &'a str,
        message: &'a str,
        parents: String,
    }

    let view = View {
        hash: commit.hash.as_str(),
        author: commit
            .author
            .as_ref()
            .and_then(|a| a.user.as_ref().map(|u| u.display_name.as_str()))
            .or_else(|| commit.author.as_ref().and_then(|a| a.raw.as_deref()))
            .unwrap_or(""),
        date: commit.date.as_deref().unwrap_or(""),
        message: commit.message.as_deref().unwrap_or(""),
        parents: commit
            .parents
            .iter()
            .map(|p| &p.hash[..7.min(p.hash.len())])
            .collect::<Vec<_>>()
            .join(", "),
    };

    ctx.renderer.render(&view)
}

pub async fn get_commit_diff(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    commit_hash: &str,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/diffstat/{commit_hash}");
    let response: DiffStat = ctx
        .client
        .get(&path)
        .await
        .with_context(|| {
            format!("Failed to fetch diff for commit {commit_hash} in {workspace}/{repo_slug}")
        })?;

    #[derive(Serialize)]
    struct Row<'a> {
        status: &'a str,
        file: &'a str,
        additions: String,
        deletions: String,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|diff| Row {
            status: diff.status.as_deref().unwrap_or("modified"),
            file: diff
                .new
                .as_ref()
                .map(|f| f.path.as_str())
                .or_else(|| diff.old.as_ref().map(|f| f.path.as_str()))
                .unwrap_or(""),
            additions: diff
                .lines_added
                .map(|n| format!("+{n}"))
                .unwrap_or_default(),
            deletions: diff
                .lines_removed
                .map(|n| format!("-{n}"))
                .unwrap_or_default(),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(commit_hash, workspace, repo_slug, "No changes in commit");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}

pub async fn browse_source(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    commit: &str,
    path: Option<&str>,
) -> Result<()> {
    let api_path = if let Some(p) = path {
        format!("/2.0/repositories/{workspace}/{repo_slug}/src/{commit}/{p}")
    } else {
        format!("/2.0/repositories/{workspace}/{repo_slug}/src/{commit}/")
    };

    let response: SourceList = ctx
        .client
        .get(&api_path)
        .await
        .with_context(|| {
            format!("Failed to browse source at {commit} in {workspace}/{repo_slug}")
        })?;

    #[derive(Serialize)]
    struct Row<'a> {
        file_type: &'a str,
        path: &'a str,
        size: String,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|file| Row {
            file_type: file.file_type.as_str(),
            path: file.path.as_str(),
            size: file
                .size
                .map(|s| format!("{s} bytes"))
                .unwrap_or_default(),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(
            commit,
            path = path.unwrap_or("/"),
            workspace,
            repo_slug,
            "No files found"
        );
        return Ok(());
    }

    ctx.renderer.render(&rows)
}
