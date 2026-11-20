//! User lookup, end to end.
//!
//! `jira issue create --assignee dev@example.com` used to send the email as an
//! account ID, and Jira answered "assignee: Please select a valid user." These
//! tests run the built binary against a mock Jira. They check the request that
//! Jira receives, because the resolved account ID in the body is the fix.

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};
use tempfile::TempDir;
use wiremock::matchers::{body_json, body_partial_json, method, path as path_matcher, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

fn write_config(dir: &Path, base_url: &str) -> std::path::PathBuf {
    let config_path = dir.join("config.yaml");
    std::fs::write(
        &config_path,
        format!(
            "default_profile: local\nprofiles:\n  local:\n    email: owner@example.com\n    base_url: {base_url}\n"
        ),
    )
    .unwrap();
    config_path
}

fn run(config: &Path, args: &[&str]) -> std::process::Output {
    let dir = config.parent().unwrap();
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        .env("HOME", dir)
        .env("ATLASSIAN_CLI_CONFIG_DIR", dir)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("RUST_LOG")
        .env_remove("ATLASSIAN_API_TOKEN")
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

fn user(account_id: &str, display_name: &str, email: Option<&str>) -> Value {
    let mut user = json!({
        "accountId": account_id,
        "accountType": "atlassian",
        "displayName": display_name,
        "active": true,
    });
    if let Some(email) = email {
        user["emailAddress"] = json!(email);
    }
    user
}

/// A mock Jira where a search for `query` returns `users`.
async fn server_with_users(query: &str, users: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/user/search"))
        .and(query_param("query", query))
        .respond_with(ResponseTemplate::new(200).set_body_json(users))
        .mount(&server)
        .await;
    server
}

fn text(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

async fn request_paths(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_resolves_an_email_assignee_to_the_account_id() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue"))
        .and(body_partial_json(
            json!({"fields": {"assignee": {"id": "acc-1"}}}),
        ))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"key": "PROJ-1", "id": "10001"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "create",
            "--project",
            "PROJ",
            "--issue-type",
            "Task",
            "--summary",
            "Check the login page",
            "--assignee",
            "dev@example.com",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PROJ-1"),
        "{}",
        text(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_with_an_unknown_email_fails_before_the_issue_is_created() {
    let server = server_with_users("nobody@example.com", json!([])).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "create",
            "--project",
            "PROJ",
            "--issue-type",
            "Task",
            "--summary",
            "Check the login page",
            "--assignee",
            "nobody@example.com",
        ],
    );
    let combined = text(&out);

    assert!(!out.status.success(), "{combined}");
    assert!(combined.contains("nobody@example.com"), "{combined}");
    assert!(combined.contains("jira user search"), "{combined}");
    assert_eq!(
        request_paths(&server).await,
        vec!["GET /rest/api/3/user/search".to_string()],
        "no issue must be created"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_with_a_plain_account_id_makes_no_lookup() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue"))
        .and(body_partial_json(
            json!({"fields": {"assignee": {"id": "acc-9"}}}),
        ))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"key": "PROJ-2", "id": "10002"})),
        )
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "create",
            "--project",
            "PROJ",
            "--issue-type",
            "Task",
            "--summary",
            "Check the login page",
            "--assignee",
            "acc-9",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        request_paths(&server).await,
        vec!["POST /rest/api/3/issue".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn assign_resolves_an_email() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    Mock::given(method("PUT"))
        .and(path_matcher("/rest/api/3/issue/PROJ-1/assignee"))
        .and(body_json(json!({"accountId": "acc-1"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "assign",
            "PROJ-1",
            "--assignee",
            "dev@example.com",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn assign_to_me_uses_the_current_user() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .respond_with(ResponseTemplate::new(200).set_body_json(user(
            "acc-me",
            "Owner",
            Some("owner@example.com"),
        )))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path_matcher("/rest/api/3/issue/PROJ-1/assignee"))
        .and(body_json(json!({"accountId": "acc-me"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "assign", "PROJ-1", "--assignee", "me"],
    );

    assert!(out.status.success(), "{}", text(&out));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn assign_with_two_matches_names_them_and_changes_nothing() {
    let server = server_with_users(
        "dev@example.com",
        json!([
            user("acc-1", "Dev One", None),
            user("acc-2", "Dev Two", None)
        ]),
    )
    .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "assign",
            "PROJ-1",
            "--assignee",
            "dev@example.com",
        ],
    );
    let combined = text(&out);

    assert!(!out.status.success(), "{combined}");
    for expected in ["Dev One", "acc-1", "Dev Two", "acc-2"] {
        assert!(combined.contains(expected), "{expected}: {combined}");
    }
    assert_eq!(
        request_paths(&server).await,
        vec!["GET /rest/api/3/user/search".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watchers_add_resolves_an_email() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue/PROJ-1/watchers"))
        .and(body_json(json!("acc-1")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "watchers",
            "add",
            "PROJ-1",
            "dev@example.com",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_assign_resolves_an_email_once_for_all_issues() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issues": [{"key": "PROJ-1"}, {"key": "PROJ-2"}]
        })))
        .mount(&server)
        .await;
    for key in ["PROJ-1", "PROJ-2"] {
        Mock::given(method("PUT"))
            .and(path_matcher(format!("/rest/api/3/issue/{key}/assignee")))
            .and(body_json(json!({"accountId": "acc-1"})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
    }
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "assign",
            "--jql",
            "project = PROJ",
            "--assignee",
            "dev@example.com",
            "--execute",
            "--yes",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
    let lookups = request_paths(&server)
        .await
        .into_iter()
        .filter(|request| request == "GET /rest/api/3/user/search")
        .count();
    assert_eq!(lookups, 1, "one lookup, not one for each issue");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_search_lists_the_matches_as_json() {
    let server = server_with_users(
        "dev",
        json!([
            user("acc-1", "Dev One", Some("dev@example.com")),
            user("acc-2", "Dev Two", None)
        ]),
    )
    .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["jira", "user", "search", "dev", "-f", "json"]);

    assert!(out.status.success(), "{}", text(&out));
    let rows: Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(
        rows,
        json!([
            {
                "account_id": "acc-1",
                "account_type": "atlassian",
                "display_name": "Dev One",
                "email": "dev@example.com",
                "active": true
            },
            {
                "account_id": "acc-2",
                "account_type": "atlassian",
                "display_name": "Dev Two",
                "email": null,
                "active": true
            }
        ])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_search_with_no_match_gives_an_empty_json_list() {
    let server = server_with_users("nobody", json!([])).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "users", "search", "nobody", "-f", "json"],
    );

    assert!(out.status.success(), "{}", text(&out));
    let rows: Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(rows, json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_search_respects_the_limit() {
    let server = server_with_users(
        "dev",
        json!([
            user("acc-1", "Dev One", None),
            user("acc-2", "Dev Two", None),
            user("acc-3", "Dev Three", None)
        ]),
    )
    .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira", "user", "search", "dev", "--limit", "2", "-f", "json",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
    let rows: Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(rows.as_array().map(Vec::len), Some(2), "{rows}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--limit"),
        "a cut list must say so on stderr: {}",
        text(&out)
    );
}

async fn mount_myself(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .respond_with(ResponseTemplate::new(200).set_body_json(user(
            "acc-me",
            "Owner",
            Some("owner@example.com"),
        )))
        .mount(server)
        .await;
}

async fn mount_issue_search(server: &MockServer, keys: &[&str]) {
    let issues: Vec<Value> = keys.iter().map(|key| json!({ "key": key })).collect();
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "issues": issues })))
        .mount(server)
        .await;
}

fn create_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "jira",
        "issue",
        "create",
        "--project",
        "PROJ",
        "--issue-type",
        "Task",
        "--summary",
        "Check the login page",
    ];
    args.extend_from_slice(extra);
    args
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_with_me_uses_the_current_user() {
    let server = MockServer::start().await;
    mount_myself(&server).await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue"))
        .and(body_partial_json(
            json!({"fields": {"assignee": {"id": "acc-me"}}}),
        ))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"key": "PROJ-1", "id": "10001"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &create_args(&["--assignee", "me"]));

    assert!(out.status.success(), "{}", text(&out));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_with_upper_case_me_uses_the_current_user() {
    let server = MockServer::start().await;
    mount_myself(&server).await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue"))
        .and(body_partial_json(
            json!({"fields": {"assignee": {"id": "acc-me"}}}),
        ))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"key": "PROJ-1", "id": "10001"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &create_args(&["--assignee", " ME"]));

    assert!(out.status.success(), "{}", text(&out));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_trims_the_assignee_before_the_lookup() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue"))
        .and(body_partial_json(
            json!({"fields": {"assignee": {"id": "acc-1"}}}),
        ))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"key": "PROJ-1", "id": "10001"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &create_args(&["--assignee", " dev@example.com "]));

    assert!(out.status.success(), "{}", text(&out));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_assignee_is_an_error_before_a_request() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &create_args(&["--assignee", "  "]));
    let combined = text(&out);

    assert!(!out.status.success(), "{combined}");
    assert!(combined.contains("--assignee"), "{combined}");
    assert!(combined.contains("empty"), "{combined}");
    assert!(request_paths(&server).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_checks_the_field_collision_before_the_lookup() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &create_args(&[
            "--assignee",
            "dev@example.com",
            "--field",
            r#"assignee={"id":"acc-9"}"#,
        ]),
    );
    let combined = text(&out);

    assert!(!out.status.success(), "{combined}");
    assert!(combined.contains("collides with --assignee"), "{combined}");
    assert!(
        request_paths(&server).await.is_empty(),
        "the collision is a local error, so no lookup must run"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_resolves_an_email_assignee_in_the_same_request() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    Mock::given(method("PUT"))
        .and(path_matcher("/rest/api/3/issue/PROJ-1"))
        .and(body_partial_json(json!({
            "fields": {"summary": "New title", "assignee": {"id": "acc-1"}}
        })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "update",
            "PROJ-1",
            "--summary",
            "New title",
            "--assignee",
            "dev@example.com",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        request_paths(&server).await,
        vec![
            "GET /rest/api/3/user/search".to_string(),
            "PUT /rest/api/3/issue/PROJ-1".to_string()
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_with_only_an_assignee_is_a_valid_update() {
    let server = MockServer::start().await;
    mount_myself(&server).await;
    Mock::given(method("PUT"))
        .and(path_matcher("/rest/api/3/issue/PROJ-1"))
        .and(body_json(json!({"fields": {"assignee": {"id": "acc-me"}}})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "update", "PROJ-1", "--assignee", "@me"],
    );

    assert!(out.status.success(), "{}", text(&out));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_checks_the_field_collision_before_the_lookup() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "update",
            "PROJ-1",
            "--assignee",
            "dev@example.com",
            "--field",
            r#"assignee={"id":"acc-9"}"#,
        ],
    );
    let combined = text(&out);

    assert!(!out.status.success(), "{combined}");
    assert!(combined.contains("collides with --assignee"), "{combined}");
    assert!(request_paths(&server).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watchers_remove_resolves_an_email() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    Mock::given(method("DELETE"))
        .and(path_matcher("/rest/api/3/issue/PROJ-1/watchers"))
        .and(query_param("accountId", "acc-1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "watchers",
            "remove",
            "PROJ-1",
            "dev@example.com",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_assign_list_mode_shows_the_account_id_and_writes_nothing() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    mount_issue_search(&server, &["PROJ-1", "PROJ-2"]).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "assign",
            "--jql",
            "project = PROJ",
            "--assignee",
            "dev@example.com",
            "-f",
            "json",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
    let rows: Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(
        rows,
        json!([
            {"key": "PROJ-1", "assignee": "acc-1", "action": "would assign"},
            {"key": "PROJ-2", "assignee": "acc-1", "action": "would assign"}
        ])
    );
    assert_eq!(
        request_paths(&server).await,
        vec![
            "GET /rest/api/3/user/search".to_string(),
            "POST /rest/api/3/search/jql".to_string()
        ],
        "one lookup, one search, no write"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_assign_with_an_unknown_email_fails_before_the_list() {
    let server = server_with_users("nobody@example.com", json!([])).await;
    mount_issue_search(&server, &["PROJ-1"]).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "assign",
            "--jql",
            "project = PROJ",
            "--assignee",
            "nobody@example.com",
            "-f",
            "json",
        ],
    );

    assert!(!out.status.success(), "{}", text(&out));
    assert!(out.stdout.is_empty(), "no list must print: {}", text(&out));
    assert_eq!(
        request_paths(&server).await,
        vec!["GET /rest/api/3/user/search".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_assign_without_yes_names_the_user_in_the_refusal() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    mount_issue_search(&server, &["PROJ-1"]).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    // The test has no terminal on stdin, so the confirmation refuses and
    // prints its warning.
    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "assign",
            "--jql",
            "project = PROJ",
            "--assignee",
            "dev@example.com",
            "--execute",
        ],
    );
    let combined = text(&out);

    assert!(!out.status.success(), "{combined}");
    assert!(combined.contains("Dev One (acc-1)"), "{combined}");
}

fn write_import_file(dir: &Path, rows: Value) -> std::path::PathBuf {
    let file = dir.join("import.json");
    std::fs::write(&file, serde_json::to_vec(&rows).unwrap()).unwrap();
    file
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_import_dry_run_makes_no_lookup() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/user/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(0)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = write_import_file(
        dir.path(),
        json!([{"summary": "Row one", "issue_type": "Task", "assignee": "dev@example.com"}]),
    );

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "import",
            "--file",
            file.to_str().unwrap(),
            "--project",
            "PROJ",
            "--dry-run",
        ],
    );

    assert!(out.status.success(), "{}", text(&out));
    assert!(request_paths(&server).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_import_creates_nothing_when_one_row_does_not_resolve() {
    let server = server_with_users(
        "dev@example.com",
        json!([user("acc-1", "Dev One", Some("dev@example.com"))]),
    )
    .await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/user/search"))
        .and(query_param("query", "nobody@example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue"))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"key": "PROJ-1", "id": "10001"})),
        )
        .expect(0)
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = write_import_file(
        dir.path(),
        json!([
            {"summary": "Row one", "issue_type": "Task", "assignee": "dev@example.com"},
            {"summary": "Row two", "issue_type": "Task", "assignee": "nobody@example.com"}
        ]),
    );

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "import",
            "--file",
            file.to_str().unwrap(),
            "--project",
            "PROJ",
        ],
    );
    let combined = text(&out);

    assert!(!out.status.success(), "{combined}");
    assert!(combined.contains("Row two"), "{combined}");
    assert!(
        !request_paths(&server)
            .await
            .contains(&"POST /rest/api/3/issue".to_string()),
        "no issue must be created"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_search_shows_the_account_type() {
    let server = server_with_users(
        "dev",
        json!([{
            "accountId": "acc-5",
            "accountType": "app",
            "displayName": "Dev Bot",
            "active": false
        }]),
    )
    .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["jira", "user", "search", "dev", "-f", "json"]);

    assert!(out.status.success(), "{}", text(&out));
    let rows: Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(
        rows,
        json!([{
            "account_id": "acc-5",
            "account_type": "app",
            "display_name": "Dev Bot",
            "email": null,
            "active": false
        }])
    );
}
