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
    #[serde(rename = "accountType", default)]
    pub account_type: Option<String>,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "emailAddress", default)]
    pub email_address: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// The account type of a person. The other documented types are `app` and
/// `customer`.
const PERSON_ACCOUNT_TYPE: &str = "atlassian";

/// A user value after the lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedUser {
    pub account_id: String,
    /// The display name, when a lookup ran and Jira returned one.
    pub display_name: Option<String>,
}

impl ResolvedUser {
    /// Text for a prompt: `Dev One (acc-1)`, or the account ID alone.
    pub(crate) fn label(&self) -> String {
        match self.display_name.as_deref() {
            Some(name) if !name.is_empty() => format!("{name} ({})", self.account_id),
            _ => self.account_id.clone(),
        }
    }
}

impl From<&JiraUser> for ResolvedUser {
    fn from(user: &JiraUser) -> Self {
        Self {
            account_id: user.account_id.clone(),
            display_name: user.display_name.clone(),
        }
    }
}

fn user_search_path(query: &str, start_at: usize, max_results: usize) -> String {
    format!(
        "/rest/api/3/user/search?query={}&startAt={start_at}&maxResults={max_results}",
        encode_query_component(query)
    )
}

/// One line for an error message: `Dev One (acc-1)`. An inactive account and
/// an account that is not a person get a tag: `(acc-1, inactive)`,
/// `(acc-2, app)`.
fn describe(user: &JiraUser) -> String {
    let mut details = vec![user.account_id.clone()];
    if user.active == Some(false) {
        details.push("inactive".to_string());
    }
    if let Some(account_type) = user.account_type.as_deref() {
        if account_type != PERSON_ACCOUNT_TYPE {
            details.push(account_type.to_string());
        }
    }
    format!(
        "  {} ({})",
        user.display_name.as_deref().unwrap_or("(no display name)"),
        details.join(", ")
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

fn is_active_person(user: &JiraUser) -> bool {
    user.active == Some(true) && user.account_type.as_deref() == Some(PERSON_ACCOUNT_TYPE)
}

/// Chooses the one user that an email address identifies. Pure, testable.
///
/// The search matches a prefix of the display name or of the email address,
/// so it can return users that the caller did not mean. The rules:
/// 1. Exactly one user with the same `emailAddress` (case ignored) wins.
/// 2. If more than one user has the same `emailAddress`, the single active
///    account of type `atlassian` among them wins.
/// 3. If no email address is the same, a single result wins only when its
///    email address is hidden. A single result that shows a different email
///    address is a different person.
/// 4. All other results are an error that names the matches.
pub(crate) fn pick_user_for_email<'a>(users: &'a [JiraUser], email: &str) -> Result<&'a JiraUser> {
    let exact: Vec<&JiraUser> = users.iter().filter(|u| same_email(u, email)).collect();
    if let [only] = exact.as_slice() {
        return Ok(only);
    }
    if exact.len() > 1 {
        let people: Vec<&JiraUser> = exact
            .iter()
            .copied()
            .filter(|u| is_active_person(u))
            .collect();
        if let [only] = people.as_slice() {
            return Ok(only);
        }
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

/// Turns a user value from the command line into a Jira user.
///
/// Spaces at the start and at the end do not count. `option` is the name of
/// the option or argument, for the error text.
///
/// - `me` (or `@me`, the form that `jira issue search --assignee` accepts),
///   in upper or lower case, gives the current user.
/// - A value that contains `@` is an email address. The user search finds it.
/// - All other values are account IDs. They pass through with no request.
pub(crate) async fn resolve_user(
    client: &ApiClient,
    value: &str,
    option: &str,
) -> Result<ResolvedUser> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{option} is empty. Give an account ID, an email address, or `me`.");
    }

    if value.eq_ignore_ascii_case("me") || value.eq_ignore_ascii_case("@me") {
        let myself: JiraUser = client
            .get("/rest/api/3/myself")
            .await
            .context("Failed to get the current Jira user for 'me'")?;
        tracing::debug!(account_id = %myself.account_id, "Resolved 'me' to the current user");
        return Ok(ResolvedUser::from(&myself));
    }

    if !value.contains('@') {
        tracing::debug!(%value, "User value has no '@'; used as an account ID");
        return Ok(ResolvedUser {
            account_id: value.to_string(),
            display_name: None,
        });
    }

    let users: Vec<JiraUser> = client
        .get(&user_search_path(value, 0, USER_PAGE_SIZE))
        .await
        .with_context(|| format!("Failed to search Jira users for '{value}'"))?;
    tracing::debug!(query = %value, matches = users.len(), "User search for an email address");

    let user = pick_user_for_email(&users, value)?;
    tracing::debug!(email = %value, account_id = %user.account_id, "Resolved an email address");
    Ok(ResolvedUser::from(user))
}

/// [`resolve_user`], for a caller that needs the account ID only.
pub(crate) async fn resolve_account_id(
    client: &ApiClient,
    value: &str,
    option: &str,
) -> Result<String> {
    Ok(resolve_user(client, value, option).await?.account_id)
}

/// [`resolve_account_id`] for an option that the caller can omit.
pub(crate) async fn resolve_optional_account_id(
    client: &ApiClient,
    value: Option<&str>,
    option: &str,
) -> Result<Option<String>> {
    match value {
        Some(value) => Ok(Some(resolve_account_id(client, value, option).await?)),
        None => Ok(None),
    }
}

/// Why a list of users is not the full result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    /// The list is the full result.
    Complete,
    /// The caller's limit cut the list. More users exist.
    Limit,
    /// The request budget stopped the search. More users can exist.
    Budget,
}

/// Fetches the users that match `query`, up to `limit` (0 gives all pages).
///
/// With a limit, the function asks for one user more than the limit. That
/// user shows that the list is cut, and the function removes it.
async fn fetch_users(
    client: &ApiClient,
    query: &str,
    limit: usize,
) -> Result<(Vec<JiraUser>, Cut)> {
    let mut users: Vec<JiraUser> = Vec::new();

    for request_index in 0..USER_PAGE_BUDGET {
        let wanted = if limit == 0 {
            USER_PAGE_SIZE
        } else {
            (limit + 1 - users.len()).min(USER_PAGE_SIZE)
        };
        let path = user_search_path(query, users.len(), wanted);
        tracing::debug!(request = %path, request_index, "Fetching a page of users");
        let page: Vec<JiraUser> = client
            .get(&path)
            .await
            .with_context(|| format!("Failed to search Jira users for '{query}'"))?;

        let received = page.len();
        users.extend(page);

        if limit > 0 && users.len() > limit {
            users.truncate(limit);
            return Ok((users, Cut::Limit));
        }
        // A page with fewer users than the request asked for is the last page.
        if received < wanted {
            return Ok((users, Cut::Complete));
        }
    }

    tracing::debug!(
        budget = USER_PAGE_BUDGET,
        collected = users.len(),
        "Stopped the user search: request budget used"
    );
    Ok((users, Cut::Budget))
}

/// `jira user search <query>`: lists users with their account IDs.
pub async fn search_users(ctx: &JiraContext<'_>, query: &str, limit: usize) -> Result<()> {
    let (users, cut) = fetch_users(&ctx.client, query, limit).await?;

    match cut {
        Cut::Complete => {}
        Cut::Limit => eprintln!(
            "warning: showing {} users; more exist. Raise --limit, or use --limit 0 for all.",
            users.len()
        ),
        Cut::Budget => eprintln!(
            "warning: showing {} users; the search stopped after {USER_PAGE_BUDGET} requests \
             and more can exist. Make the query more specific.",
            users.len()
        ),
    }

    #[derive(Serialize)]
    struct Row<'a> {
        account_id: &'a str,
        display_name: &'a str,
        email: Option<&'a str>,
        account_type: Option<&'a str>,
        active: Option<bool>,
    }

    let rows: Vec<Row<'_>> = users
        .iter()
        .map(|user| Row {
            account_id: user.account_id.as_str(),
            display_name: user.display_name.as_deref().unwrap_or(""),
            email: user.email_address.as_deref(),
            account_type: user.account_type.as_deref(),
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
        resolve_account_id(&client, value, "--assignee").await
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

        let (users, cut) = fetch_users(&client, "dev", 0).await.unwrap();

        assert_eq!(users.len(), USER_PAGE_SIZE + 1);
        assert_eq!(users.last().unwrap().account_id, "acc-last");
        assert_eq!(cut, Cut::Complete);
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

        let (users, cut) = fetch_users(&client, "dev", 2).await.unwrap();
        assert_eq!(users.len(), 2);
        assert_eq!(cut, Cut::Limit);

        let (users, cut) = fetch_users(&client, "dev", 3).await.unwrap();
        assert_eq!(users.len(), 3);
        assert_eq!(cut, Cut::Complete, "a short page is the last page");
    }

    fn typed_user(account_id: &str, account_type: &str, active: bool) -> Value {
        json!({
            "accountId": account_id,
            "accountType": account_type,
            "displayName": format!("User {account_id}"),
            "emailAddress": "dev@example.com",
            "active": active,
        })
    }

    #[tokio::test]
    async fn the_one_active_person_wins_among_users_with_the_same_email() {
        let server = server_with_users(
            "dev@example.com",
            json!([
                typed_user("acc-old", "atlassian", false),
                typed_user("acc-bot", "app", true),
                typed_user("acc-1", "atlassian", true)
            ]),
        )
        .await;

        assert_eq!(resolve(&server, "dev@example.com").await.unwrap(), "acc-1");
    }

    #[tokio::test]
    async fn two_active_people_with_the_same_email_is_an_error_with_tags() {
        let server = server_with_users(
            "dev@example.com",
            json!([
                typed_user("acc-1", "atlassian", true),
                typed_user("acc-2", "atlassian", true),
                typed_user("acc-old", "atlassian", false),
                typed_user("acc-bot", "app", true)
            ]),
        )
        .await;

        let error = resolve(&server, "dev@example.com").await.unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("User acc-1 (acc-1)"), "{message}");
        assert!(
            message.contains("User acc-old (acc-old, inactive)"),
            "{message}"
        );
        assert!(message.contains("User acc-bot (acc-bot, app)"), "{message}");
    }

    #[tokio::test]
    async fn me_ignores_case_and_outer_spaces() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/myself"))
            .respond_with(ResponseTemplate::new(200).set_body_json(user("acc-me", "Owner", None)))
            .mount(&server)
            .await;

        for value in ["ME", " me", "@Me ", "Me"] {
            assert_eq!(
                resolve(&server, value).await.unwrap(),
                "acc-me",
                "{value:?}"
            );
        }
    }

    #[tokio::test]
    async fn outer_spaces_do_not_go_into_the_query_or_the_account_id() {
        let server = server_with_users(
            "dev@example.com",
            json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
        )
        .await;

        assert_eq!(
            resolve(&server, " dev@example.com ").await.unwrap(),
            "acc-1"
        );
        assert_eq!(resolve(&server, " acc-9 ").await.unwrap(), "acc-9");
    }

    #[tokio::test]
    async fn an_empty_value_is_an_error_that_names_the_option_with_no_request() {
        let server = MockServer::start().await;

        for value in ["", "   "] {
            let error = resolve(&server, value).await.unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains("--assignee is empty"), "{message}");
        }
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn resolve_user_gives_the_display_name_for_a_prompt() {
        let server = server_with_users(
            "dev@example.com",
            json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
        )
        .await;
        let client = ApiClient::new(server.uri()).unwrap();

        let found = resolve_user(&client, "dev@example.com", "--assignee")
            .await
            .unwrap();
        assert_eq!(found.label(), "Dev One (acc-1)");

        let plain = resolve_user(&client, "acc-9", "--assignee").await.unwrap();
        assert_eq!(plain.label(), "acc-9");
    }

    fn many_users(count: usize) -> Value {
        let users: Vec<Value> = (0..count)
            .map(|i| user(&format!("acc-{i}"), "Dev", None))
            .collect();
        json!(users)
    }

    #[tokio::test]
    async fn fetch_users_asks_for_one_more_than_the_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/user/search"))
            .and(query_param("startAt", "0"))
            .and(query_param("maxResults", "4"))
            .respond_with(ResponseTemplate::new(200).set_body_json(many_users(3)))
            .mount(&server)
            .await;
        let client = ApiClient::new(server.uri()).unwrap();

        let (users, cut) = fetch_users(&client, "dev", 3).await.unwrap();

        assert_eq!(users.len(), 3);
        assert_eq!(cut, Cut::Complete, "the result is exactly the limit");
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn fetch_users_with_exactly_one_full_page_is_complete() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/user/search"))
            .and(query_param("startAt", "0"))
            .and(query_param("maxResults", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(many_users(50)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/user/search"))
            .and(query_param("startAt", "50"))
            .and(query_param("maxResults", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let client = ApiClient::new(server.uri()).unwrap();

        let (users, cut) = fetch_users(&client, "dev", 50).await.unwrap();

        assert_eq!(users.len(), 50);
        assert_eq!(cut, Cut::Complete);
        assert_eq!(request_count(&server).await, 2);
    }

    #[tokio::test]
    async fn fetch_users_reports_the_request_budget() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/user/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(many_users(USER_PAGE_SIZE)))
            .mount(&server)
            .await;
        let client = ApiClient::new(server.uri()).unwrap();

        let (users, cut) = fetch_users(&client, "dev", 0).await.unwrap();

        assert_eq!(cut, Cut::Budget);
        assert_eq!(users.len(), USER_PAGE_SIZE * USER_PAGE_BUDGET);
        assert_eq!(request_count(&server).await, USER_PAGE_BUDGET);
    }
}
