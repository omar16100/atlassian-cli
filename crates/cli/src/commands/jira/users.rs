//! Jira users: the account ID resolver and `jira user search`.
//!
//! Jira Cloud identifies a user by account ID only. A person knows an email
//! address. This module turns an email address, or the literal `me`, into the
//! account ID before a command sends it.
//!
//! Endpoints (Jira Cloud platform REST API v3):
//! - `GET /rest/api/3/user/search?query=<text>` ("Find users"). It returns a
//!   bare JSON array of users, paged by `startAt` and `maxResults`.
//! - `GET /rest/api/3/myself` ("Get current user").

use anyhow::{bail, Context, Result};
use atlassian_cli_api::query::encode_query_component;
use atlassian_cli_api::ApiClient;
use serde::{Deserialize, Serialize};

use super::utils::JiraContext;

/// The page size of "Find users". It is the documented default of
/// `maxResults`.
const USER_PAGE_SIZE: usize = 50;

/// The maximum number of requests for one `jira user search`. The endpoint
/// documents that it does not go past the thousandth user (20 pages of 50).
const USER_PAGE_BUDGET: usize = 20;

/// The maximum number of matches that an error message names.
const MAX_NAMED_MATCHES: usize = 10;

/// The fields of a Jira user that this module reads.
///
/// Privacy settings can remove `emailAddress`, so it is optional.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct JiraUser {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "emailAddress", default)]
    pub email_address: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

fn user_search_path(query: &str, start_at: usize, max_results: usize) -> String {
    format!(
        "/rest/api/3/user/search?query={}&startAt={start_at}&maxResults={max_results}",
        encode_query_component(query)
    )
}

fn describe(user: &JiraUser) -> String {
    format!(
        "  {} (account ID: {})",
        user.display_name.as_deref().unwrap_or("(no display name)"),
        user.account_id
    )
}

fn describe_all(users: &[&JiraUser]) -> String {
    let mut lines: Vec<String> = users
        .iter()
        .take(MAX_NAMED_MATCHES)
        .map(|user| describe(user))
        .collect();
    if users.len() > MAX_NAMED_MATCHES {
        lines.push(format!("  and {} more", users.len() - MAX_NAMED_MATCHES));
    }
    lines.join("\n")
}

fn same_email(user: &JiraUser, email: &str) -> bool {
    user.email_address
        .as_deref()
        .is_some_and(|found| found.eq_ignore_ascii_case(email))
}

/// Chooses the one user that an email address identifies. Pure, testable.
///
/// The search matches a prefix of the display name or of the email address,
/// so it can return users that the caller did not mean. The rules:
/// 1. Exactly one user with the same `emailAddress` (case ignored) wins.
/// 2. If no email address is the same, a single result wins only when its
///    email address is hidden. A single result that shows a different email
///    address is a different person.
/// 3. All other results are an error that names the matches.
pub(crate) fn pick_user_for_email<'a>(users: &'a [JiraUser], email: &str) -> Result<&'a JiraUser> {
    let exact: Vec<&JiraUser> = users.iter().filter(|u| same_email(u, email)).collect();
    if let [only] = exact.as_slice() {
        return Ok(only);
    }
    if exact.len() > 1 {
        bail!(
            "{} Jira users have the email address '{email}':\n{}\n\
             Pass the account ID of the correct user.",
            exact.len(),
            describe_all(&exact)
        );
    }

    match users {
        [] => bail!(
            "No Jira user matches '{email}'.\n\
             The privacy settings of a user can hide the email address from \
             the Jira API.\n\
             Jira also returns no users when your account does not have the \
             'Browse users and groups' global permission.\n\
             Search by name with `atlassian-cli jira user search <query>`, \
             then pass the account ID."
        ),
        [only] if only.email_address.is_none() => Ok(only),
        [only] => bail!(
            "No Jira user has the email address '{email}'. The nearest match is:\n{}\n\
             Pass the account ID if this is the correct user.",
            describe(only)
        ),
        many => {
            let all: Vec<&JiraUser> = many.iter().collect();
            bail!(
                "'{email}' matches {} Jira users, and none of them shows that \
                 exact email address:\n{}\n\
                 Pass the account ID of the correct user.",
                many.len(),
                describe_all(&all)
            )
        }
    }
}

/// Turns a user value from the command line into a Jira account ID.
///
/// - `me` (or `@me`, the form that `jira issue search --assignee` accepts)
///   gives the account ID of the current user.
/// - A value that contains `@` is an email address. The user search finds it.
/// - All other values are account IDs. They pass through with no request.
pub(crate) async fn resolve_account_id(client: &ApiClient, value: &str) -> Result<String> {
    if value == "me" || value == "@me" {
        let myself: JiraUser = client
            .get("/rest/api/3/myself")
            .await
            .context("Failed to get the current Jira user for 'me'")?;
        tracing::debug!(account_id = %myself.account_id, "Resolved 'me' to the current user");
        return Ok(myself.account_id);
    }

    if !value.contains('@') {
        tracing::debug!(%value, "User value has no '@'; used as an account ID");
        return Ok(value.to_string());
    }

    let users: Vec<JiraUser> = client
        .get(&user_search_path(value, 0, USER_PAGE_SIZE))
        .await
        .with_context(|| format!("Failed to search Jira users for '{value}'"))?;
    tracing::debug!(query = %value, matches = users.len(), "User search for an email address");

    let user = pick_user_for_email(&users, value)?;
    tracing::debug!(email = %value, account_id = %user.account_id, "Resolved an email address");
    Ok(user.account_id.clone())
}

/// Fetches the users that match `query`, up to `limit` (0 gives all pages).
///
/// The second value is true when more users can exist than the list holds.
async fn fetch_users(
    client: &ApiClient,
    query: &str,
    limit: usize,
) -> Result<(Vec<JiraUser>, bool)> {
    let mut users: Vec<JiraUser> = Vec::new();

    for page_index in 0..USER_PAGE_BUDGET {
        let path = user_search_path(query, page_index * USER_PAGE_SIZE, USER_PAGE_SIZE);
        tracing::debug!(request = %path, page_index, "Fetching a page of users");
        let page: Vec<JiraUser> = client
            .get(&path)
            .await
            .with_context(|| format!("Failed to search Jira users for '{query}'"))?;

        let full_page = page.len() >= USER_PAGE_SIZE;
        users.extend(page);

        if limit > 0 && users.len() >= limit {
            let truncated = users.len() > limit || full_page;
            users.truncate(limit);
            return Ok((users, truncated));
        }
        // A page with fewer users than the page size is the last page.
        if !full_page {
            return Ok((users, false));
        }
    }

    tracing::warn!(
        budget = USER_PAGE_BUDGET,
        collected = users.len(),
        "Stopped the user search: request budget used"
    );
    Ok((users, true))
}

/// `jira user search <query>`: lists users with their account IDs.
pub async fn search_users(ctx: &JiraContext<'_>, query: &str, limit: usize) -> Result<()> {
    let (users, truncated) = fetch_users(&ctx.client, query, limit).await?;

    if truncated {
        eprintln!(
            "warning: showing {} users; more can exist. Raise --limit, or make the query more specific.",
            users.len()
        );
    }

    #[derive(Serialize)]
    struct Row<'a> {
        account_id: &'a str,
        display_name: &'a str,
        email: Option<&'a str>,
        active: Option<bool>,
    }

    let rows: Vec<Row<'_>> = users
        .iter()
        .map(|user| Row {
            account_id: user.account_id.as_str(),
            display_name: user.display_name.as_deref().unwrap_or(""),
            email: user.email_address.as_deref(),
            active: user.active,
        })
        .collect();

    ctx.renderer.render_list_or_empty(&rows, "No users found")
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn user(account_id: &str, display_name: &str, email: Option<&str>) -> Value {
        let mut user = json!({
            "accountId": account_id,
            "displayName": display_name,
            "active": true,
        });
        if let Some(email) = email {
            user["emailAddress"] = json!(email);
        }
        user
    }

    async fn server_with_users(query: &str, users: Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/user/search"))
            .and(query_param("query", query))
            .respond_with(ResponseTemplate::new(200).set_body_json(users))
            .mount(&server)
            .await;
        server
    }

    async fn resolve(server: &MockServer, value: &str) -> Result<String> {
        let client = ApiClient::new(server.uri()).unwrap();
        resolve_account_id(&client, value).await
    }

    async fn request_count(server: &MockServer) -> usize {
        server.received_requests().await.unwrap_or_default().len()
    }

    #[tokio::test]
    async fn one_match_gives_its_account_id() {
        let server = server_with_users(
            "dev@example.com",
            json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
        )
        .await;

        assert_eq!(resolve(&server, "dev@example.com").await.unwrap(), "acc-1");
    }

    #[tokio::test]
    async fn one_match_with_a_hidden_email_gives_its_account_id() {
        let server =
            server_with_users("dev@example.com", json!([user("acc-1", "Dev One", None)])).await;

        assert_eq!(resolve(&server, "dev@example.com").await.unwrap(), "acc-1");
    }

    #[tokio::test]
    async fn a_null_email_address_is_a_hidden_email() {
        let server = server_with_users(
            "dev@example.com",
            json!([{"accountId": "acc-1", "displayName": "Dev One", "emailAddress": null}]),
        )
        .await;

        assert_eq!(resolve(&server, "dev@example.com").await.unwrap(), "acc-1");
    }

    #[tokio::test]
    async fn one_match_with_a_different_email_is_an_error() {
        // The search matches a prefix, so `dev@example.com` finds this user.
        let server = server_with_users(
            "dev@example.com",
            json!([user("acc-7", "Dev Seven", Some("dev@example.com.test"))]),
        )
        .await;

        let error = resolve(&server, "dev@example.com").await.unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("Dev Seven"), "{message}");
        assert!(message.contains("acc-7"), "{message}");
    }

    #[tokio::test]
    async fn zero_matches_is_an_error_that_names_the_search_command() {
        let server = server_with_users("nobody@example.com", json!([])).await;

        let error = resolve(&server, "nobody@example.com").await.unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("nobody@example.com"), "{message}");
        assert!(message.contains("privacy settings"), "{message}");
        assert!(message.contains("jira user search <query>"), "{message}");
    }

    #[tokio::test]
    async fn many_matches_is_an_error_that_names_each_match() {
        let server = server_with_users(
            "dev@example.com",
            json!([
                user("acc-1", "Dev One", None),
                user("acc-2", "Dev Two", None)
            ]),
        )
        .await;

        let error = resolve(&server, "dev@example.com").await.unwrap_err();
        let message = format!("{error:#}");
        for expected in ["Dev One", "acc-1", "Dev Two", "acc-2"] {
            assert!(message.contains(expected), "{expected}: {message}");
        }
    }

    #[tokio::test]
    async fn an_exact_email_among_many_matches_wins() {
        let server = server_with_users(
            "dev@example.com",
            json!([
                user("acc-1", "Dev One", Some("dev@example.com.test")),
                user("acc-2", "Dev Two", Some("Dev@Example.com")),
                user("acc-3", "Dev Three", None)
            ]),
        )
        .await;

        assert_eq!(resolve(&server, "dev@example.com").await.unwrap(), "acc-2");
    }

    #[tokio::test]
    async fn two_users_with_the_same_email_is_an_error() {
        let server = server_with_users(
            "dev@example.com",
            json!([
                user("acc-1", "Dev One", Some("dev@example.com")),
                user("acc-2", "Dev Two", Some("dev@example.com"))
            ]),
        )
        .await;

        let error = resolve(&server, "dev@example.com").await.unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("acc-1") && message.contains("acc-2"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn me_gives_the_current_user() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/myself"))
            .respond_with(ResponseTemplate::new(200).set_body_json(user("acc-me", "Owner", None)))
            .mount(&server)
            .await;

        assert_eq!(resolve(&server, "me").await.unwrap(), "acc-me");
        assert_eq!(resolve(&server, "@me").await.unwrap(), "acc-me");
    }

    #[tokio::test]
    async fn a_plain_account_id_makes_no_request() {
        let server = MockServer::start().await;

        assert_eq!(resolve(&server, "acc-1").await.unwrap(), "acc-1");
        assert_eq!(resolve(&server, "5b10:ab-cd").await.unwrap(), "5b10:ab-cd");
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn the_query_is_percent_encoded() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;

        let _ = resolve(&server, "a+b&x=1@example.com").await;

        let requests = server.received_requests().await.unwrap();
        assert_eq!(
            requests[0].url.query(),
            Some("query=a%2Bb%26x%3D1%40example.com&startAt=0&maxResults=50")
        );
    }

    #[tokio::test]
    async fn fetch_users_follows_full_pages_and_stops_on_a_short_page() {
        let server = MockServer::start().await;
        let full: Vec<Value> = (0..USER_PAGE_SIZE)
            .map(|i| user(&format!("acc-{i}"), "Dev", None))
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/api/3/user/search"))
            .and(query_param("startAt", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(full)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/user/search"))
            .and(query_param("startAt", "50"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!([user("acc-last", "Dev", None)])),
            )
            .mount(&server)
            .await;
        let client = ApiClient::new(server.uri()).unwrap();

        let (users, truncated) = fetch_users(&client, "dev", 0).await.unwrap();

        assert_eq!(users.len(), USER_PAGE_SIZE + 1);
        assert_eq!(users.last().unwrap().account_id, "acc-last");
        assert!(!truncated);
        assert_eq!(request_count(&server).await, 2);
    }

    #[tokio::test]
    async fn fetch_users_cuts_at_the_limit_and_reports_it() {
        let server = server_with_users(
            "dev",
            json!([
                user("acc-1", "Dev One", None),
                user("acc-2", "Dev Two", None),
                user("acc-3", "Dev Three", None)
            ]),
        )
        .await;
        let client = ApiClient::new(server.uri()).unwrap();

        let (users, truncated) = fetch_users(&client, "dev", 2).await.unwrap();
        assert_eq!(users.len(), 2);
        assert!(truncated);

        let (users, truncated) = fetch_users(&client, "dev", 3).await.unwrap();
        assert_eq!(users.len(), 3);
        assert!(!truncated, "a short page is the last page");
    }
}
