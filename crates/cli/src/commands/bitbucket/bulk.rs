use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::{encode_ref_path, BitbucketContext};
use crate::commands::common::confirm_destructive;

#[derive(Deserialize)]
struct RepositoryList {
    values: Vec<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    slug: String,
    name: String,
    #[serde(default)]
    updated_on: Option<String>,
}

#[derive(Deserialize)]
struct BranchList {
    values: Vec<Branch>,
}

#[derive(Deserialize)]
struct Branch {
    name: String,
    #[serde(default)]
    target: Option<Target>,
}

#[derive(Deserialize)]
struct Target {
    #[serde(default)]
    date: Option<String>,
}

pub async fn archive_stale_repos(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    days_threshold: i64,
    dry_run: bool,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}?pagelen=100");
    let response: RepositoryList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list repositories in workspace {workspace}"))?;

    let now = chrono::Utc::now();
    let threshold = chrono::Duration::days(days_threshold);

    #[derive(Serialize)]
    struct StaleRepo<'a> {
        slug: &'a str,
        name: &'a str,
        last_updated: &'a str,
        action: &'a str,
    }

    let mut stale_repos = Vec::new();

    for repo in &response.values {
        if let Some(updated) = &repo.updated_on {
            if let Ok(updated_date) = chrono::DateTime::parse_from_rfc3339(updated) {
                let age = now.signed_duration_since(updated_date);
                if age > threshold {
                    stale_repos.push(StaleRepo {
                        slug: repo.slug.as_str(),
                        name: repo.name.as_str(),
                        last_updated: updated,
                        action: if dry_run { "would archive" } else { "archived" },
                    });

                    if !dry_run {
                        let update_path = format!("/2.0/repositories/{workspace}/{}", repo.slug);
                        let payload = serde_json::json!({
                            "has_issues": false,
                            "has_wiki": false,
                        });

                        let _: serde_json::Value = ctx
                            .client
                            .put(&update_path, &payload)
                            .await
                            .with_context(|| {
                                format!("Failed to archive repository {}", repo.slug)
                            })?;

                        tracing::info!(
                            repo_slug = repo.slug.as_str(),
                            workspace,
                            "Repository archived"
                        );
                    }
                }
            }
        }
    }

    if dry_run {
        println!(
            "DRY RUN - No changes made. Found {} stale repositories:",
            stale_repos.len()
        );
    }

    ctx.renderer.render_list_or_empty(
        &stale_repos,
        "No stale repositories found (threshold: {days_threshold} days)",
    )
}

/// Branch names never deleted, whatever the filter says.
const PROTECTED_BRANCHES: [&str; 4] = ["main", "master", "develop", "development"];

/// Decide whether a branch is shielded from deletion.
///
/// Split out so the selection rule is testable without HTTP. Note what it does
/// *not* consult: merge status. See `delete_branches` for why.
pub(crate) fn is_protected(name: &str, exclude_patterns: &[String]) -> bool {
    PROTECTED_BRANCHES.contains(&name)
        || exclude_patterns
            .iter()
            .any(|pattern| name.contains(pattern))
}

/// Delete branches selected purely by name.
///
/// This was called `delete_merged_branches` and advertised as "Delete merged
/// branches". It never checked merge status, and still does not: Bitbucket's
/// `/refs/branches` response carries no merge information, and establishing it
/// costs either a pull request cross-reference or a request per branch. The
/// command deletes every branch that is not in `PROTECTED_BRANCHES` and not
/// matched by `--exclude`, which includes live feature and release branches.
///
/// It also examines only the first 100 branches: the request sets `pagelen=100`
/// and never follows `next`. The listing, the confirmation count and the
/// deletions are all capped at that first page. Fixing this is deliberately
/// sequenced after the safety gate, because removing the cap without the gate
/// would have widened the blast radius rather than narrowing it.
///
/// Rather than quietly narrow a command people may already depend on, the
/// behaviour is unchanged and the safety rails are what changed:
///
/// - the name and help text now say what it actually does,
/// - listing is the default and deleting requires `--execute`,
/// - `--execute` requires typing the repository slug, or `--yes`.
///
/// The old `--dry-run` flag is still accepted and now redundant, so existing
/// invocations that passed it keep working and mean the same thing. Existing
/// invocations that omitted it used to delete and now list instead, which is
/// the intended change.
pub async fn delete_branches(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    exclude_patterns: Vec<String>,
    execute: bool,
    assume_yes: bool,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/refs/branches?pagelen=100");
    let response: BranchList = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list branches for {workspace}/{repo_slug}"))?;

    #[derive(Serialize)]
    struct DeletableBranch<'a> {
        name: &'a str,
        last_commit: &'a str,
        action: &'a str,
    }

    let targets: Vec<&Branch> = response
        .values
        .iter()
        .filter(|branch| !is_protected(&branch.name, &exclude_patterns))
        .collect();

    // Confirm before the first delete, not after some of them have happened.
    if execute && !assume_yes && !targets.is_empty() {
        confirm_destructive(
            repo_slug,
            &format!(
                "About to delete {} branch(es) from {workspace}/{repo_slug}.\n\
                 Merge status is NOT checked: unmerged branches will be deleted.\n\
                 Only the first 100 branches were examined.\n\
                 Re-run without --execute to list them first.",
                targets.len()
            ),
        )?;
    }

    let mut rows = Vec::with_capacity(targets.len());
    let mut failure: Option<anyhow::Error> = None;

    for branch in targets {
        if execute {
            let delete_path = format!(
                "/2.0/repositories/{workspace}/{repo_slug}/refs/branches/{}",
                encode_ref_path(&branch.name)
            );
            let result: Result<serde_json::Value> =
                ctx.client.delete(&delete_path).await.with_context(|| {
                    format!(
                        "Failed to delete branch {} from {workspace}/{repo_slug}",
                        branch.name
                    )
                });

            // Stop on the first failure, but do not discard the record of what
            // was already deleted. Returning `?` here would abort before
            // rendering, leaving the user with an error naming one branch and
            // no way to learn which others are already gone.
            if let Err(err) = result {
                failure = Some(err);
                break;
            }

            tracing::info!(
                branch_name = branch.name.as_str(),
                workspace,
                repo_slug,
                "Branch deleted"
            );
        }

        rows.push(DeletableBranch {
            name: branch.name.as_str(),
            last_commit: branch
                .target
                .as_ref()
                .and_then(|t| t.date.as_deref())
                .unwrap_or(""),
            action: if execute { "deleted" } else { "would delete" },
        });
    }

    if let Some(err) = failure {
        if !rows.is_empty() {
            eprintln!(
                "Stopped after an error. {} branch(es) were already deleted:",
                rows.len()
            );
            ctx.renderer.render_list(&rows)?;
        }
        return Err(err);
    }

    if !execute && !rows.is_empty() {
        // stderr, so a piped `-f json` listing stays machine-readable.
        eprintln!(
            "Listing only. {} branch(es) match; merge status was not checked. \
             Pass --execute to delete them.",
            rows.len()
        );
    }

    ctx.renderer
        .render_list_or_empty(&rows, "No branches matched (excluding protected patterns)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn protected_branches_are_never_selected() {
        for name in PROTECTED_BRANCHES {
            assert!(is_protected(name, &[]), "{name} should be protected");
        }
    }

    #[test]
    fn exclude_patterns_match_as_substrings() {
        let excludes = patterns(&["release/"]);
        assert!(is_protected("release/2026-09", &excludes));
        assert!(!is_protected("feature/login", &excludes));
    }

    /// The defect this command was rewritten for: an unmerged feature branch is
    /// selected for deletion, because nothing here consults merge status. The
    /// test pins the behaviour so the rename cannot be mistaken for a fix.
    #[test]
    fn unmerged_feature_branches_are_still_selected() {
        assert!(!is_protected("feature/work-in-progress", &[]));
        assert!(!is_protected("hotfix/urgent", &patterns(&["release/"])));
    }

    use atlassian_cli_api::ApiClient;
    use atlassian_cli_output::{OutputFormat, OutputRenderer};
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Serve a branch listing containing `names`, and accept any DELETE.
    async fn server_with_branches(names: &[&str]) -> MockServer {
        let server = MockServer::start().await;
        let values: Vec<_> = names
            .iter()
            .map(|n| serde_json::json!({ "name": n, "target": { "date": "2026-01-01" } }))
            .collect();

        Mock::given(method("GET"))
            .and(path_regex(r".*/refs/branches$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": values
            })))
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        server
    }

    fn ctx_for(server: &MockServer, renderer: &OutputRenderer) -> BitbucketContext<'static> {
        // `renderer` outlives every call below; the transmute-free way to say so
        // is to leak it, which is acceptable in a test process.
        let renderer: &'static OutputRenderer =
            Box::leak(Box::new(OutputRenderer::new(renderer.format())));
        BitbucketContext {
            client: ApiClient::new(server.uri()).unwrap(),
            renderer,
            is_bearer: false,
        }
    }

    async fn delete_paths(server: &MockServer) -> Vec<String> {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.method == wiremock::http::Method::DELETE)
            .map(|r| r.url.path().to_string())
            .collect()
    }

    /// The single most important test in this file. If someone makes deletion
    /// the default again, or drops the `execute` guard, this fails.
    #[tokio::test]
    async fn listing_is_the_default_and_deletes_nothing() {
        let server = server_with_branches(&["main", "feature/a", "feature/b"]).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        delete_branches(&ctx, "ws", "repo", vec![], false, false)
            .await
            .unwrap();

        assert!(
            delete_paths(&server).await.is_empty(),
            "listing must not issue any DELETE"
        );
    }

    #[tokio::test]
    async fn execute_with_yes_deletes_only_unprotected_branches() {
        let server = server_with_branches(&["main", "develop", "feature/a"]).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        delete_branches(&ctx, "ws", "repo", vec![], true, true)
            .await
            .unwrap();

        let deleted = delete_paths(&server).await;
        assert_eq!(deleted.len(), 1, "only the unprotected branch: {deleted:?}");
        assert!(deleted[0].ends_with("/refs/branches/feature/a"));
    }

    /// A branch named `main#old` must not resolve to a DELETE on `main`.
    /// Without percent-encoding, `#` starts a URL fragment and the request
    /// lands on the protected ref instead.
    #[tokio::test]
    async fn a_hash_in_a_branch_name_cannot_delete_the_protected_ref() {
        let server = server_with_branches(&["main", "main#old"]).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        delete_branches(&ctx, "ws", "repo", vec![], true, true)
            .await
            .unwrap();

        let deleted = delete_paths(&server).await;
        assert_eq!(deleted.len(), 1, "exactly one delete: {deleted:?}");
        assert!(
            !deleted[0].ends_with("/refs/branches/main"),
            "must not have deleted the protected ref: {deleted:?}"
        );
        assert!(
            deleted[0].contains("main%23old"),
            "expected the encoded ref, got: {deleted:?}"
        );
    }

    #[tokio::test]
    async fn exclude_patterns_are_honoured_under_execute() {
        let server = server_with_branches(&["release/1", "feature/a"]).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        delete_branches(&ctx, "ws", "repo", vec!["release/".to_string()], true, true)
            .await
            .unwrap();

        let deleted = delete_paths(&server).await;
        assert_eq!(deleted.len(), 1, "{deleted:?}");
        assert!(deleted[0].ends_with("/refs/branches/feature/a"));
    }

    #[test]
    fn protection_is_exact_for_names_and_substring_for_patterns() {
        // "main" is protected, but "maintenance" is not a protected name.
        assert!(is_protected("main", &[]));
        assert!(!is_protected("maintenance", &[]));
        // As a pattern, though, "main" would shield it.
        assert!(is_protected("maintenance", &patterns(&["main"])));
    }
}
