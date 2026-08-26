//! End-to-end tests for `--fields` on `jira issue get` and `jira issue search`,
//! driving the real binary against a mock Jira.
//!
//! What matters here is the request the *command* builds and the output it
//! prints: which ids reach the `fields` parameter, whether the field list is
//! fetched at all, and whether JSON stays lossless while the tabular formats
//! flatten. None of that is visible from a test that drives `ApiClient`.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{method, path as path_matcher, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

fn write_config(dir: &Path, base_url: &str) -> std::path::PathBuf {
    let config_path = dir.join("config.yaml");
    std::fs::write(
        &config_path,
        format!(
            "default_profile: local\nprofiles:\n  local:\n    email: dev@example.com\n    base_url: {base_url}\n"
        ),
    )
    .unwrap();
    config_path
}

fn run(config: &Path, args: &[&str]) -> std::process::Output {
    let dir = config.parent().unwrap_or_else(|| Path::new("."));
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        .env("HOME", dir)
        .env("ATLASSIAN_CLI_CONFIG_DIR", dir)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("ATLASSIAN_API_TOKEN")
        .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
        .env_remove("BITBUCKET_TOKEN")
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn assert_ok(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `/rest/api/3/field`, with two fields sharing a display name so the ambiguity
/// path is reachable from the same fixture.
async fn mock_field_list(server: &MockServer, times: u64) {
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/field"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "summary", "name": "Summary", "custom": false},
            {"id": "status", "name": "Status", "custom": false},
            {"id": "customfield_10016", "name": "Story Points", "custom": true},
            {"id": "customfield_10099", "name": "Team", "custom": true},
            {"id": "customfield_10100", "name": "Team", "custom": true},
        ])))
        .expect(times)
        .mount(server)
        .await;
}

fn issue_body(fields: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "key": "DEV-1", "id": "10001", "fields": fields })
}

#[tokio::test]
async fn get_with_fields_requests_only_the_resolved_ids() {
    let server = MockServer::start().await;
    mock_field_list(&server, 1).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .and(query_param("fields", "status,customfield_10016"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(issue_body(serde_json::json!({
                "status": {"name": "In Progress"},
                "customfield_10016": 5,
            }))),
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
            "get",
            "DEV-1",
            "--fields",
            "status,Story Points",
        ],
    );

    assert_ok(&out);
    let text = stdout(&out);
    assert!(text.contains("In Progress"), "{text}");
    assert!(text.contains('5'), "{text}");
}

/// The fast path: nothing but custom field ids, so the field list is not worth
/// a round trip.
#[tokio::test]
async fn get_with_custom_field_ids_skips_the_field_lookup() {
    let server = MockServer::start().await;
    mock_field_list(&server, 0).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .and(query_param("fields", "customfield_10016"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(issue_body(serde_json::json!({ "customfield_10016": 8 }))),
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
            "get",
            "DEV-1",
            "--fields",
            "customfield_10016",
        ],
    );

    assert_ok(&out);
}

#[tokio::test]
async fn get_all_asks_jira_for_every_field() {
    let server = MockServer::start().await;
    mock_field_list(&server, 0).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .and(query_param("fields", "*all"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(issue_body(serde_json::json!({
                "summary": "Everything",
                "customfield_10016": 3,
            }))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "get", "DEV-1", "--fields", "all"],
    );

    assert_ok(&out);
    assert!(stdout(&out).contains("Everything"));
}

/// Without the flag, nothing changes: no `fields` parameter, and the curated
/// view is what prints.
#[tokio::test]
async fn get_without_fields_sends_no_fields_parameter() {
    let server = MockServer::start().await;
    mock_field_list(&server, 0).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .and(query_param_is_missing("fields"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(issue_body(serde_json::json!({
                "summary": "Curated",
                "status": {"name": "Open"},
                "reporter": {"displayName": "Ada"},
            }))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "get", "DEV-1", "--format", "markdown"],
    );

    assert_ok(&out);
    let text = stdout(&out);
    // The curated markdown view, which --fields deliberately does not produce.
    assert!(text.contains("| Reporter |"), "{text}");
    assert!(text.contains("## Description"), "{text}");
}

/// JSON must be exactly what Jira sent. Flattening there would make the format
/// people pipe into `jq` the lossy one.
#[tokio::test]
async fn get_json_keeps_the_raw_field_value() {
    let server = MockServer::start().await;
    // `customfield_10099` is already an id, so no field list is fetched.
    mock_field_list(&server, 0).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .and(query_param("fields", "customfield_10099"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue_body(
            serde_json::json!({ "customfield_10099": {"value": "Internal", "id": "42"} }),
        )))
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
            "get",
            "DEV-1",
            "--fields",
            "customfield_10099",
            "--format",
            "json",
        ],
    );

    assert_ok(&out);
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("stdout is JSON");
    assert_eq!(
        parsed["customfield_10099"],
        serde_json::json!({"value": "Internal", "id": "42"}),
        "the wrapper object should survive untouched"
    );
    assert_eq!(parsed["key"], serde_json::json!("DEV-1"));
}

/// The same payload in a table shows the label, not the JSON.
#[tokio::test]
async fn get_table_flattens_a_wrapper_object_to_its_label() {
    let server = MockServer::start().await;
    // `customfield_10099` is already an id, so no field list is fetched.
    mock_field_list(&server, 0).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .and(query_param("fields", "customfield_10099"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue_body(
            serde_json::json!({ "customfield_10099": {"value": "Internal", "id": "42"} }),
        )))
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
            "get",
            "DEV-1",
            "--fields",
            "customfield_10099",
        ],
    );

    assert_ok(&out);
    let text = stdout(&out);
    assert!(text.contains("Internal"), "{text}");
    assert!(!text.contains("{\"value\""), "raw JSON in a table: {text}");
}

/// An unknown name is caught before the issue is fetched, so the user is not
/// billed a request to be told they made a typo.
#[tokio::test]
async fn an_unknown_field_name_fails_before_any_issue_request() {
    let server = MockServer::start().await;
    mock_field_list(&server, 1).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue_body(serde_json::json!({}))))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "get", "DEV-1", "--fields", "stroy points"],
    );

    assert!(!out.status.success(), "an unknown field should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("stroy points"), "{stderr}");
    assert!(stderr.contains("jira fields list"), "{stderr}");
}

/// Two fields named "Team" hold different values, so guessing is not an option.
#[tokio::test]
async fn an_ambiguous_field_name_lists_the_candidate_ids() {
    let server = MockServer::start().await;
    mock_field_list(&server, 1).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue_body(serde_json::json!({}))))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "get", "DEV-1", "--fields", "Team"],
    );

    assert!(!out.status.success(), "an ambiguous field should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ambiguous"), "{stderr}");
    assert!(stderr.contains("customfield_10099"), "{stderr}");
    assert!(stderr.contains("customfield_10100"), "{stderr}");
}

fn search_response() -> serde_json::Value {
    serde_json::json!({
        "issues": [
            {
                "key": "DEV-1",
                "fields": {
                    "summary": "First",
                    "status": {"name": "Open"},
                    "customfield_10016": 5,
                }
            },
            {
                "key": "DEV-2",
                "fields": {
                    "summary": "Second",
                    "status": {"name": "Done"},
                }
            }
        ],
        "isLast": true
    })
}

/// The default five columns, unchanged.
#[tokio::test]
async fn search_without_fields_keeps_the_default_columns() {
    let server = MockServer::start().await;
    mock_field_list(&server, 0).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .and(query_param(
            "fields",
            "key,summary,status,assignee,issuetype",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_response()))
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
            "search",
            "--jql",
            "project = DEV",
            "--format",
            "csv",
        ],
    );

    assert_ok(&out);
    let text = stdout(&out);
    let header = text.lines().next().unwrap_or_default();
    assert_eq!(header, "assignee,issue_type,key,status,summary", "{text}");
}

/// `--fields` replaces those columns, and the order typed is the order printed.
/// Alphabetical sorting would have put `key,status,Story Points` in a different
/// order every time the selection changed.
#[tokio::test]
async fn search_with_fields_replaces_the_columns_and_keeps_their_order() {
    let server = MockServer::start().await;
    mock_field_list(&server, 1).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .and(query_param("fields", "status,customfield_10016"))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_response()))
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
            "search",
            "--jql",
            "project = DEV",
            "--fields",
            "status,Story Points",
            "--format",
            "csv",
        ],
    );

    assert_ok(&out);
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    assert_eq!(lines[0], "key,status,Story Points");
    assert_eq!(lines[1], "DEV-1,Open,5");
    // DEV-2 has no story points: an empty cell, not a missing column.
    assert_eq!(lines[2], "DEV-2,Done,");
}

#[tokio::test]
async fn search_with_fields_json_keeps_raw_values() {
    let server = MockServer::start().await;
    mock_field_list(&server, 1).await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .and(query_param("fields", "status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_response()))
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
            "search",
            "--jql",
            "project = DEV",
            "--fields",
            "status",
            "--format",
            "json",
        ],
    );

    assert_ok(&out);
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("stdout is JSON");
    assert_eq!(
        parsed[0]["status"],
        serde_json::json!({"name": "Open"}),
        "the status object should survive untouched"
    );
}
