//! Silent truncation, end to end.
//!
//! The reported symptom was that a query returning exactly the server's page
//! size was indistinguishable from a complete result: the output looked
//! authoritative and was wrong. Jira's `/search/jql` caps `maxResults` at 100
//! however large a value is sent, so `--limit 250` returned 100 with nothing to
//! say so.
//!
//! These drive the built binary against a mock Jira, because what a caller
//! actually receives on stdout is the thing under test. Both search paths are
//! covered: the default columns and the `--fields` path, which was worse -- it
//! never parsed the page cursor at all, and it is the documented workaround for
//! the fields `issue get` drops, so a user escaping one defect landed in
//! another.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{method, path as path_matcher, query_param};
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
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        .arg("--config-dir")
        .arg(config.parent().unwrap())
        .env("HOME", config.parent().unwrap())
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("ATLASSIAN_API_TOKEN")
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

fn issue(key: &str) -> serde_json::Value {
    serde_json::json!({
        "key": key,
        "fields": {
            "summary": format!("summary for {key}"),
            "status": {"name": "Open"},
            "issuetype": {"name": "Task"}
        }
    })
}

/// Serve two pages of search results, the first carrying a `nextPageToken`.
async fn two_page_search() -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .and(query_param("nextPageToken", "page-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issues": [issue("DEV-3")],
            "isLast": true
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issues": [issue("DEV-1"), issue("DEV-2")],
            "nextPageToken": "page-2",
            "isLast": false
        })))
        .mount(&server)
        .await;

    server
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_search_spanning_pages_returns_every_issue() {
    let server = two_page_search().await;
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
            "--limit",
            "50",
            "-f",
            "json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    for key in ["DEV-1", "DEV-2", "DEV-3"] {
        assert!(
            stdout.contains(key),
            "{key} missing -- the second page was dropped: {stdout}"
        );
    }
}

/// The `--fields` path never parsed the cursor at all, so it could not even
/// detect the boundary the default path saw and ignored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_fields_path_also_spans_pages() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/field"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "summary", "name": "Summary"},
            {"id": "status", "name": "Status"}
        ])))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .and(query_param("nextPageToken", "page-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issues": [issue("DEV-3")],
            "isLast": true
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issues": [issue("DEV-1")],
            "nextPageToken": "page-2",
            "isLast": false
        })))
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
            "summary",
            "--limit",
            "50",
            "-f",
            "json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("DEV-3"),
        "the --fields path dropped the second page: {stdout}"
    );
}

/// A capped result must say so. Silence here is the original complaint: a
/// truncated answer that looks complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_capped_search_warns_on_stderr() {
    let server = two_page_search().await;
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
            "--limit",
            "1",
            "-f",
            "json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "{stdout}");
    assert!(
        stderr.contains("more match this query"),
        "a truncated result must announce itself: {stderr}"
    );
    // The warning goes to stderr so `| jq` still works.
    assert!(
        !stdout.contains("more match this query"),
        "the warning must not pollute stdout: {stdout}"
    );
    assert!(stdout.contains("DEV-1"));
    assert!(
        !stdout.contains("DEV-3"),
        "the limit must still be honoured: {stdout}"
    );
}

/// A single-page result is complete, and must not be labelled otherwise.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_complete_search_does_not_warn() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issues": [issue("DEV-1")],
            "isLast": true
        })))
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
            "-f",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success());
    assert!(
        !stderr.contains("more match this query"),
        "a complete result must not claim to be truncated: {stderr}"
    );
}

/// The stderr warning is a stopgap for the tabular formats. A machine consumer
/// needs the signal *in* the output, which is what `--envelope` is for: a bare
/// array cannot say "there is more".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_envelope_carries_the_truncation_signal() {
    let server = two_page_search().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "--envelope",
            "jira",
            "issue",
            "search",
            "--jql",
            "project = DEV",
            "--limit",
            "1",
            "-f",
            "json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");

    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));

    assert_eq!(parsed["truncated"], serde_json::json!(true), "{stdout}");
    assert_eq!(parsed["count"], serde_json::json!(1), "{stdout}");
    assert!(parsed["data"].is_array(), "{stdout}");
    assert!(
        parsed.get("next").is_some(),
        "a truncated result should say where it stopped: {stdout}"
    );
}

/// The counterpart: a complete result must not claim truncation, and must not
/// invent a total Jira never reported.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_complete_enveloped_result_is_not_marked_truncated() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issues": [issue("DEV-1")],
            "isLast": true
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "--envelope",
            "jira",
            "issue",
            "search",
            "--jql",
            "project = DEV",
            "-f",
            "json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(parsed["truncated"], serde_json::json!(false), "{stdout}");
    assert!(
        parsed.get("total").is_none(),
        "Jira reports no total; the envelope must not invent one: {stdout}"
    );
    assert!(parsed.get("next").is_none(), "{stdout}");
}
