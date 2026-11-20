//! End-to-end tests for the 404 hint of `confluence page get`, `update` and
//! `publish`. They run the built binary against a mock Confluence.
//!
//! A folder ID given to a page command got "Resource not found" and the hint
//! "Check if the resource ID is correct". The ID was correct, it was the
//! command that was wrong. On a 404 the command now asks the folder endpoint
//! one time. The tests also check that a successful page read, and an error
//! that is not a 404, make no folder request.

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
    let dir = config.parent().unwrap_or_else(|| Path::new("."));
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        .env("HOME", dir)
        .env("ATLASSIAN_CLI_CONFIG_DIR", dir)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("RUST_LOG")
        .env_remove("ATLASSIAN_API_TOKEN")
        .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
        .env_remove("BITBUCKET_TOKEN")
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

/// Mount the page GET with `status`, expected exactly one time.
async fn mock_page(server: &MockServer, id: &str, status: u16, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path_matcher(format!("/wiki/api/v2/pages/{id}")))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(1)
        .mount(server)
        .await;
}

/// Mount the folder GET with `status`, expected exactly `calls` times.
async fn mock_folder(server: &MockServer, id: &str, status: u16, calls: u64) {
    let body = if status == 200 {
        serde_json::json!({ "id": id, "type": "folder", "title": "Architecture" })
    } else {
        serde_json::json!({ "errors": [{ "status": status, "title": "Not available" }] })
    };
    Mock::given(method("GET"))
        .and(path_matcher(format!("/wiki/api/v2/folders/{id}")))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(calls)
        .mount(server)
        .await;
}

fn not_found_body() -> serde_json::Value {
    serde_json::json!({ "errors": [{ "status": 404, "code": "NOT_FOUND", "title": "Not Found" }] })
}

#[tokio::test]
async fn a_successful_page_get_makes_no_folder_request() {
    let server = MockServer::start().await;
    // The query is part of the match, so a lost `body-format` fails the test.
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/1001"))
        .and(query_param("body-format", "storage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "id": "1001", "title": "Onboarding", "status": "current" }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    mock_folder(&server, "1001", 200, 0).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &["confluence", "page", "get", "1001"]);

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Onboarding"));
    assert!(
        out.stderr.is_empty() || !String::from_utf8_lossy(&out.stderr).contains("Hint"),
        "no hint on success: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    server.verify().await;
}

#[tokio::test]
async fn a_folder_id_names_the_folder_command() {
    let server = MockServer::start().await;
    mock_page(&server, "2002", 404, not_found_body()).await;
    mock_folder(&server, "2002", 200, 1).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &["confluence", "page", "get", "2002"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "a 404 must fail");
    assert!(out.stdout.is_empty(), "nothing on stdout: {:?}", out.stdout);
    assert!(
        stderr.contains("Hint: 2002 is a folder, not a page."),
        "{stderr}"
    );
    assert!(
        stderr.contains("atlassian-cli confluence folder get 2002"),
        "{stderr}"
    );
    // The first error stays visible.
    assert!(
        stderr.contains("Error: Failed to get page 2002: Resource not found"),
        "{stderr}"
    );
    // The general hint is replaced, not added.
    assert!(
        !stderr.contains("Check if the resource ID is correct"),
        "{stderr}"
    );
    server.verify().await;
}

#[tokio::test]
async fn an_unknown_id_names_the_probable_causes() {
    let server = MockServer::start().await;
    mock_page(&server, "3003", 404, not_found_body()).await;
    mock_folder(&server, "3003", 404, 1).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &["confluence", "page", "get", "3003"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(
        stderr.contains("Hint: Page 3003 was not found."),
        "{stderr}"
    );
    assert!(stderr.contains("the ID is wrong"), "{stderr}");
    assert!(stderr.contains("no permission"), "{stderr}");
    assert!(stderr.contains("not a page"), "{stderr}");
    assert!(!stderr.contains("is a folder"), "{stderr}");
    assert!(
        stderr.contains("Error: Failed to get page 3003: Resource not found"),
        "{stderr}"
    );
    server.verify().await;
}

/// A failed probe must not replace the page error with its own error.
#[tokio::test]
async fn a_forbidden_folder_probe_keeps_the_page_error() {
    let server = MockServer::start().await;
    mock_page(&server, "4004", 404, not_found_body()).await;
    mock_folder(&server, "4004", 403, 1).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &["confluence", "page", "get", "4004"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(
        stderr.contains("Hint: Page 4004 was not found."),
        "{stderr}"
    );
    assert!(
        stderr.contains("Error: Failed to get page 4004: Resource not found"),
        "{stderr}"
    );
    assert!(!stderr.contains("forbidden"), "{stderr}");
    assert!(!stderr.contains("is a folder"), "{stderr}");
    server.verify().await;
}

/// Only a 404 starts the probe. A 403 on the page keeps its own hint.
#[tokio::test]
async fn a_forbidden_page_makes_no_folder_request() {
    let server = MockServer::start().await;
    mock_page(
        &server,
        "5005",
        403,
        serde_json::json!({ "message": "no access" }),
    )
    .await;
    mock_folder(&server, "5005", 200, 0).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &["confluence", "page", "get", "5005"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(stderr.contains("Access forbidden"), "{stderr}");
    assert!(!stderr.contains("is a folder"), "{stderr}");
    server.verify().await;
}

/// `page update` reads the page first, with the same helper.
#[tokio::test]
async fn page_update_on_a_folder_id_names_the_folder_command() {
    let server = MockServer::start().await;
    mock_page(&server, "6006", 404, not_found_body()).await;
    mock_folder(&server, "6006", 200, 1).await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(
        &config,
        &["confluence", "page", "update", "6006", "--title", "New"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(
        stderr.contains("atlassian-cli confluence folder get 6006"),
        "{stderr}"
    );
    server.verify().await;
}

/// `page publish` reads the page first, with the same helper.
#[tokio::test]
async fn page_publish_on_a_folder_id_names_the_folder_command() {
    let server = MockServer::start().await;
    mock_page(&server, "7007", 404, not_found_body()).await;
    mock_folder(&server, "7007", 200, 1).await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let body = dir.path().join("body.html");
    std::fs::write(&body, "<p>Text</p>").unwrap();
    let out = run(
        &config,
        &[
            "confluence",
            "page",
            "publish",
            "7007",
            "--body",
            body.to_str().unwrap(),
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(
        stderr.contains("atlassian-cli confluence folder get 7007"),
        "{stderr}"
    );
    server.verify().await;
}

/// The probe only improves a hint, so it is sent one time. A 503 from the
/// folder endpoint is not tried again, and no retry warning is printed.
#[tokio::test]
async fn a_failing_folder_probe_is_not_retried() {
    let server = MockServer::start().await;
    mock_page(&server, "8008", 404, not_found_body()).await;
    mock_folder(&server, "8008", 503, 1).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &["confluence", "page", "get", "8008"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(
        stderr.contains("Hint: Page 8008 was not found."),
        "{stderr}"
    );
    assert!(
        stderr.contains("Error: Failed to get page 8008: Resource not found"),
        "{stderr}"
    );
    assert!(!stderr.contains("retry"), "{stderr}");
    assert!(!stderr.contains("WARN"), "{stderr}");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    server.verify().await;
}

/// `page get` sends the ID as one path segment. A value with `/` is refused
/// before a request is sent.
#[tokio::test]
async fn an_id_with_a_slash_sends_no_request() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["confluence", "page", "get", "a/b"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(stderr.contains("Invalid identifier"), "{stderr}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A query marker in the ID stays in the path segment. It does not reach the
/// query string of the request.
#[tokio::test]
async fn an_id_with_a_query_marker_is_one_path_segment() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/123%3Fversion%3D2"))
        .and(query_param("body-format", "storage"))
        .respond_with(ResponseTemplate::new(404).set_body_json(not_found_body()))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &["confluence", "page", "get", "123?version=2"]);

    assert!(!out.status.success());
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|r| !r.url.query().unwrap_or("").contains("version")),
        "the ID must not reach the query string"
    );
    server.verify().await;
}
