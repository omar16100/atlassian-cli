use anyhow::{bail, Context, Result};
use atlassian_cli_api::pagination::{fetch_paged, BitbucketPage, PageLimits};
use serde::{Deserialize, Serialize};

use super::utils::{encode_ref_path, BitbucketContext};
use crate::commands::common::confirm_destructive;

#[derive(Deserialize)]
struct Repository {
    slug: String,
    name: String,
    #[serde(default)]
    updated_on: Option<String>,
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

/// Decide whether a repository counts as stale.
///
/// Extracted so the threshold arithmetic is testable without HTTP or a clock.
/// A repository with no `updated_on` is never stale: absent data is not
/// evidence of disuse, and treating it as such would mutate repositories on the
/// strength of a missing field.
pub(crate) fn is_stale(
    updated_on: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
    days: i64,
) -> bool {
    let Some(updated) = updated_on else {
        return false;
    };
    let Ok(updated_date) = chrono::DateTime::parse_from_rfc3339(updated) else {
        // Not stale, but say so. A server-side date-format change would
        // otherwise turn this command into a permanent "No stale repositories
        // found" with nothing to explain why.
        tracing::warn!(
            updated_on = updated,
            "Ignoring repository: last-updated timestamp is not valid RFC 3339"
        );
        return false;
    };
    // `try_days` rather than `days`: the latter panics on a value large enough
    // to overflow, and `--days` comes from the command line.
    let Some(threshold) = chrono::Duration::try_days(days) else {
        tracing::warn!(
            days,
            "Staleness threshold is out of range; treating nothing as stale"
        );
        return false;
    };
    now.signed_duration_since(updated_date) > threshold
}

/// Disable the issue tracker and wiki on stale repositories.
///
/// This was called "archive stale repositories" and reported `archived`. It
/// does not archive anything: Bitbucket Cloud has no repository archive API,
/// and the `PUT` sets only `has_issues: false` and `has_wiki: false`. The
/// label described an operation that never happened, while the operation that
/// did happen — turning off two features, hiding any issues and wiki pages
/// filed in them — went unnamed.
///
/// As with `delete_branches`, the behaviour is kept and the description and
/// rails are what changed: listing is the default, `--execute` performs the
/// change, and `--execute` needs the workspace typed back or `--yes`.
///
/// The listing is followed to completion; a list that cannot be completed
/// within the request budget is an error rather than a silent truncation,
/// because it drives mutations.
pub async fn disable_features_on_stale_repos(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    days_threshold: i64,
    execute: bool,
    assume_yes: bool,
) -> Result<()> {
    let path = format!("/2.0/repositories/{workspace}?pagelen=100");
    let (repositories, page) =
        fetch_paged::<BitbucketPage<Repository>>(&ctx.client, &path, PageLimits::new(None))
            .await
            .with_context(|| format!("Failed to list repositories in workspace {workspace}"))?;

    // Same reasoning as delete_branches: this list drives mutations.
    if page.truncated {
        bail!(
            "Refusing to continue: only {} repositories could be listed before the request \
             budget ran out, so the set shown would be incomplete.",
            repositories.len()
        );
    }

    let now = chrono::Utc::now();

    #[derive(Serialize)]
    struct StaleRepo<'a> {
        slug: &'a str,
        name: &'a str,
        last_updated: &'a str,
        action: &'a str,
    }

    let targets: Vec<&Repository> = repositories
        .iter()
        .filter(|repo| is_stale(repo.updated_on.as_deref(), now, days_threshold))
        .collect();

    if execute && !assume_yes && !targets.is_empty() {
        confirm_destructive(
            workspace,
            &format!(
                "About to disable the issue tracker and wiki on {} repositor(ies) in {workspace}.\n\
                 This is not archiving: existing issues and wiki pages become inaccessible.\n\
                 Re-run without --execute to list them first.",
                targets.len()
            ),
        )?;
    }

    let mut rows = Vec::with_capacity(targets.len());
    let mut failure: Option<anyhow::Error> = None;

    for repo in targets {
        if execute {
            let update_path = format!("/2.0/repositories/{workspace}/{}", repo.slug);
            let payload = serde_json::json!({
                "has_issues": false,
                "has_wiki": false,
            });

            let result: Result<serde_json::Value> = ctx
                .client
                .put(&update_path, &payload)
                .await
                .with_context(|| format!("Failed to disable features on repository {}", repo.slug));

            // Same reasoning as delete_branches: report what already changed
            // rather than aborting with only the failing name.
            if let Err(err) = result {
                failure = Some(err);
                break;
            }

            tracing::info!(
                repo_slug = repo.slug.as_str(),
                workspace,
                "Issue tracker and wiki disabled"
            );
        }

        rows.push(StaleRepo {
            slug: repo.slug.as_str(),
            name: repo.name.as_str(),
            last_updated: repo.updated_on.as_deref().unwrap_or(""),
            action: if execute {
                "issues and wiki disabled"
            } else {
                "would disable issues and wiki"
            },
        });
    }

    if let Some(err) = failure {
        if !rows.is_empty() {
            eprintln!(
                "Stopped after an error. {} repositor(ies) were already changed:",
                rows.len()
            );
            ctx.renderer.render_list(&rows)?;
        }
        return Err(err);
    }

    if !execute && !rows.is_empty() {
        eprintln!(
            "Listing only. {} repositor(ies) match. Pass --execute to disable \
             their issue tracker and wiki.",
            rows.len()
        );
    }

    ctx.renderer.render_list_or_empty(
        &rows,
        // Was a plain string containing a literal `{days_threshold}`, which
        // printed verbatim because it was never a format string.
        &format!("No stale repositories found (threshold: {days_threshold} days)"),
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
/// The listing is now followed to completion rather than capped at the first
/// page. That cap was previously the only bound on the blast radius, which is
/// why the safety gate landed first: removing it before the gate existed would
/// have widened the damage rather than narrowing it. A list that cannot be
/// completed within the request budget is an error here, not a truncation.
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
    let (branches, page) =
        fetch_paged::<BitbucketPage<Branch>>(&ctx.client, &path, PageLimits::new(None))
            .await
            .with_context(|| format!("Failed to list branches for {workspace}/{repo_slug}"))?;

    // Everywhere else a truncated list is labelled and rendered. Here it is an
    // error: an incomplete branch list feeds deletions, and "these are the
    // branches, probably" is not something to confirm against.
    if page.truncated {
        bail!(
            "Refusing to continue: only {} branches could be listed before the request \
             budget ran out, so the set shown would be incomplete.",
            branches.len()
        );
    }

    #[derive(Serialize)]
    struct DeletableBranch<'a> {
        name: &'a str,
        last_commit: &'a str,
        action: &'a str,
    }

    let targets: Vec<&Branch> = branches
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
                 Re-run without --execute to list them first.",
                targets.len()
            ),
        )?;
    }

    let mut rows = Vec::with_capacity(targets.len());
    let mut failure: Option<anyhow::Error> = None;

    for branch in targets {
        if execute {
            // Not `?`: a rejected name this far into the loop means earlier
            // branches are already deleted, and returning here would discard
            // that record -- the exact thing the failure handling below exists
            // to prevent.
            let encoded = match encode_ref_path(&branch.name) {
                Ok(encoded) => encoded,
                Err(err) => {
                    failure = Some(err);
                    break;
                }
            };
            let delete_path =
                format!("/2.0/repositories/{workspace}/{repo_slug}/refs/branches/{encoded}");
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

    /// A repository with no `updated_on` must never be treated as stale:
    /// acting on a missing field would mutate repositories on no evidence.
    #[test]
    fn missing_updated_on_is_never_stale() {
        let now = chrono::Utc::now();
        assert!(!is_stale(None, now, 180));
        assert!(!is_stale(Some("not-a-date"), now, 180));
    }

    /// Exactly at the threshold is not stale: the comparison is strict.
    #[test]
    fn the_threshold_boundary_is_exclusive() {
        let now = chrono::Utc::now();
        let exactly = (now - chrono::Duration::days(180)).to_rfc3339();
        assert!(!is_stale(Some(&exactly), now, 180));

        let a_moment_older =
            (now - chrono::Duration::days(180) - chrono::Duration::seconds(1)).to_rfc3339();
        assert!(is_stale(Some(&a_moment_older), now, 180));
    }

    /// `chrono::Duration::days` panics on an overflowing value, and `--days`
    /// is user input.
    #[test]
    fn an_absurd_threshold_does_not_panic() {
        let now = chrono::Utc::now();
        let old = (now - chrono::Duration::days(500)).to_rfc3339();
        assert!(!is_stale(Some(&old), now, i64::MAX));
    }

    #[test]
    fn staleness_compares_against_the_threshold() {
        let now = chrono::Utc::now();
        let old = (now - chrono::Duration::days(200)).to_rfc3339();
        let recent = (now - chrono::Duration::days(10)).to_rfc3339();
        assert!(is_stale(Some(&old), now, 180));
        assert!(!is_stale(Some(&recent), now, 180));
    }

    async fn server_with_repos(slugs_and_dates: &[(&str, String)]) -> MockServer {
        let server = MockServer::start().await;
        let values: Vec<_> = slugs_and_dates
            .iter()
            .map(|(slug, date)| {
                serde_json::json!({ "slug": slug, "name": slug, "updated_on": date })
            })
            .collect();

        Mock::given(method("GET"))
            .and(path_regex(r"^/2\.0/repositories/[^/]+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": values
            })))
            .mount(&server)
            .await;

        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        server
    }

    async fn put_count(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.method == wiremock::http::Method::PUT)
            .count()
    }

    /// The archive-repos equivalent of the gate test below: without --execute
    /// nothing is mutated.
    #[tokio::test]
    async fn stale_repo_listing_mutates_nothing() {
        let old = (chrono::Utc::now() - chrono::Duration::days(400)).to_rfc3339();
        let server = server_with_repos(&[("dusty", old)]).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        disable_features_on_stale_repos(&ctx, "ws", 180, false, false)
            .await
            .unwrap();

        assert_eq!(put_count(&server).await, 0, "listing must not PUT");
    }

    #[tokio::test]
    async fn execute_disables_features_only_on_stale_repos() {
        let now = chrono::Utc::now();
        let old = (now - chrono::Duration::days(400)).to_rfc3339();
        let fresh = (now - chrono::Duration::days(3)).to_rfc3339();
        let server = server_with_repos(&[("dusty", old), ("busy", fresh)]).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        disable_features_on_stale_repos(&ctx, "ws", 180, true, true)
            .await
            .unwrap();

        assert_eq!(put_count(&server).await, 1, "only the stale repository");
    }

    /// The truncation defect this branch set out to fix, on the destructive
    /// path: a second page of branches must be seen, not silently dropped.
    #[tokio::test]
    async fn every_page_of_branches_is_listed() {
        let server = MockServer::start().await;
        let page_two = format!(
            "{}/2.0/repositories/ws/repo/refs/branches?page=2",
            server.uri()
        );

        Mock::given(method("GET"))
            .and(path_regex(r".*/refs/branches$"))
            .and(wiremock::matchers::query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [{"name": "second-page", "target": {"date": "2026-01-01"}}]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r".*/refs/branches$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [{"name": "first-page", "target": {"date": "2026-01-01"}}],
                "next": page_two
            })))
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        delete_branches(&ctx, "ws", "repo", vec![], true, true)
            .await
            .unwrap();

        let deleted = delete_paths(&server).await;
        assert_eq!(deleted.len(), 2, "both pages must be acted on: {deleted:?}");
        assert!(deleted.iter().any(|p| p.ends_with("/second-page")));
    }

    /// A list that cannot be completed must abort rather than confirm against a
    /// partial set. Everywhere else truncation is a label; here it is an error.
    #[tokio::test]
    async fn an_incompletable_branch_list_refuses_to_delete() {
        let server = MockServer::start().await;
        let forever = format!(
            "{}/2.0/repositories/ws/repo/refs/branches?page=next",
            server.uri()
        );

        Mock::given(method("GET"))
            .and(path_regex(r".*/refs/branches$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [{"name": "a", "target": {"date": "2026-01-01"}}],
                "next": forever
            })))
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        let err = delete_branches(&ctx, "ws", "repo", vec![], true, true)
            .await
            .expect_err("an incomplete listing must not proceed to deletion");

        assert!(
            format!("{err:#}").contains("Refusing to continue"),
            "unexpected error: {err:#}"
        );
        assert!(
            delete_paths(&server).await.is_empty(),
            "nothing may be deleted from an incomplete list"
        );
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

    /// End-to-end guard for the traversal hole: a hostile listing entry must
    /// abort the run, not issue a DELETE against the retargeted path.
    #[tokio::test]
    async fn a_traversal_branch_name_aborts_without_deleting() {
        let server = server_with_branches(&["a/../../../../../../repositories/w2/r2"]).await;
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        let err = delete_branches(&ctx, "ws", "repo", vec![], true, true)
            .await
            .expect_err("a dot-segment ref name must be refused");

        assert!(
            format!("{err:#}").contains("would change which resource"),
            "unexpected error: {err:#}"
        );
        assert!(
            delete_paths(&server).await.is_empty(),
            "nothing may be deleted when a name is refused"
        );
    }

    /// A hostile name in a *later* slot must not discard the record of the
    /// deletions that already happened. The earlier test only covered a
    /// rejected name in the first position, where there is nothing to lose.
    #[tokio::test]
    async fn a_late_traversal_name_still_reports_earlier_deletions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(r".*/refs/branches$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [
                    {"name": "aaa", "target": {"date": "2026-01-01"}},
                    {"name": "x/../../../../../../repositories/w2/r2",
                     "target": {"date": "2026-01-01"}}
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        let err = delete_branches(&ctx, "ws", "repo", vec![], true, true)
            .await
            .expect_err("the hostile name must abort the run");
        assert!(
            format!("{err:#}").contains("would change which resource"),
            "unexpected error: {err:#}"
        );

        let deleted = delete_paths(&server).await;
        assert_eq!(deleted.len(), 1, "only the safe branch: {deleted:?}");
        assert!(deleted[0].ends_with("/aaa"));
        assert!(
            !deleted.iter().any(|p| p.contains("/repositories/w2/r2")),
            "the retargeted path must never be requested: {deleted:?}"
        );
    }

    /// A failure part-way through must still report what was already deleted,
    /// rather than aborting with only the failing name.
    #[tokio::test]
    async fn a_partial_failure_reports_what_was_deleted() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(r".*/refs/branches$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "values": [
                    {"name": "aaa", "target": {"date": "2026-01-01"}},
                    {"name": "bbb", "target": {"date": "2026-01-01"}}
                ]
            })))
            .mount(&server)
            .await;
        // First branch succeeds, second is refused by the server.
        Mock::given(method("DELETE"))
            .and(path_regex(r".*/refs/branches/aaa$"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path_regex(r".*/refs/branches/bbb$"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let ctx = ctx_for(&server, &renderer);

        delete_branches(&ctx, "ws", "repo", vec![], true, true)
            .await
            .expect_err("the second delete fails");

        let deleted = delete_paths(&server).await;
        assert!(
            deleted.iter().any(|p| p.ends_with("/aaa")),
            "the first delete really happened: {deleted:?}"
        );
        assert!(
            deleted.iter().any(|p| p.ends_with("/bbb")),
            "the failing delete was attempted: {deleted:?}"
        );
        // Count distinct branches, not requests: a 500 is retryable, so the
        // client legitimately asks for `bbb` more than once.
        let mut distinct: Vec<&str> = deleted
            .iter()
            .filter_map(|p| p.rsplit('/').next())
            .collect();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct,
            vec!["aaa", "bbb"],
            "must stop at the failure rather than continuing: {deleted:?}"
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
