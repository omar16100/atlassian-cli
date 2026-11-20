//! End-to-end tests for `confluence page add-restriction`. They run the built
//! binary against a mock Confluence.
//!
//! The expected request comes from the Confluence Cloud REST API v1
//! specification (`POST /wiki/rest/api/content/{id}/restriction`, schema
//! `ContentRestrictionAddOrUpdateArray`),
//! <https://dac-static.atlassian.com/cloud/confluence/swagger.v3.json>.
//! It is not verified against a live Confluence site.
//!
//! The command sent one bare object with `identifier` fields. The
//! specification requires a `results` array, `accountId` for a user and `id`
//! for a group.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{body_json, method, path as path_matcher};
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

fn add_args<'a>(operation: &'a str, subject_type: &'a str, subject_id: &'a str) -> Vec<&'a str> {
    vec![
        "confluence",
        "page",
        "add-restriction",
        "1001",
        "--operation",
        operation,
        "--subject-type",
        subject_type,
        "--subject-id",
        subject_id,
    ]
}

/// Mount the POST with an exact body, expected one time.
async fn mock_add(server: &MockServer, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path_matcher("/wiki/rest/api/content/1001/restriction"))
        .and(body_json(body))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "results": [] })),
        )
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn add_restriction_for_a_user_sends_the_documented_body() {
    let server = MockServer::start().await;
    mock_add(
        &server,
        serde_json::json!({
            "results": [{
                "operation": "update",
                "restrictions": {
                    "user": { "results": [{ "accountId": "acc-1" }] }
                }
            }]
        }),
    )
    .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &add_args("update", "user", "acc-1"));

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout)
        .contains("Added update restriction for acc-1 to page 1001"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    server.verify().await;
}

#[tokio::test]
async fn add_restriction_for_a_group_sends_the_group_id() {
    let server = MockServer::start().await;
    mock_add(
        &server,
        serde_json::json!({
            "results": [{
                "operation": "read",
                "restrictions": {
                    "group": { "results": [{ "id": "grp-1" }] }
                }
            }]
        }),
    )
    .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &add_args("read", "group", "grp-1"));

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    server.verify().await;
}

/// A value outside the documented set is refused before a request is sent.
#[tokio::test]
async fn an_unknown_operation_or_subject_type_sends_no_request() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    // The values are case-sensitive, so `Read` is refused too.
    for bad_operation in ["delete", "Read"] {
        let out = run(&config, &add_args(bad_operation, "user", "acc-1"));
        assert!(!out.status.success(), "{bad_operation}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("--operation"), "{stderr}");
        assert!(
            stderr.contains("[possible values: read, update]"),
            "{stderr}"
        );
    }

    for bad_subject in ["team", "User"] {
        let out = run(&config, &add_args("read", bad_subject, "acc-1"));
        assert!(!out.status.success(), "{bad_subject}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("--subject-type"), "{stderr}");
        assert!(
            stderr.contains("[possible values: user, group]"),
            "{stderr}"
        );
    }

    assert!(server.received_requests().await.unwrap().is_empty());
}

/// `--help` names the permitted values.
#[test]
fn help_lists_the_permitted_values() {
    let out = Command::new(BIN)
        .args(["confluence", "page", "add-restriction", "--help"])
        .output()
        .expect("failed to run the CLI");
    // clap wraps the help text, so compare with the white space collapsed.
    let stdout = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(out.status.success());
    assert!(
        stdout.contains("[possible values: read, update]"),
        "{stdout}"
    );
    assert!(
        stdout.contains("[possible values: user, group]"),
        "{stdout}"
    );
}

/// An empty or blank `--subject-id` names no subject. No request is sent.
#[tokio::test]
async fn an_empty_subject_id_sends_no_request() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    for blank in ["", "   "] {
        let out = run(&config, &add_args("read", "user", blank));
        assert!(!out.status.success(), "{blank:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("--subject-id must not be empty"),
            "{stderr}"
        );
    }

    assert!(server.received_requests().await.unwrap().is_empty());
}

/// The value that is checked is the value that is sent: spaces at the two ends
/// are removed.
#[tokio::test]
async fn the_subject_id_is_sent_trimmed() {
    let server = MockServer::start().await;
    mock_add(
        &server,
        serde_json::json!({
            "results": [{
                "operation": "read",
                "restrictions": {
                    "user": { "results": [{ "accountId": "acc-1" }] }
                }
            }]
        }),
    )
    .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &add_args("read", "user", "  acc-1 "));

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    server.verify().await;
}

/// A 404 from the server (no such content, or no permission) is a clear error
/// and a non-zero exit. Nothing is written to stdout.
#[tokio::test]
async fn a_404_from_the_server_is_a_clear_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/wiki/rest/api/content/1001/restriction"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "statusCode": 404,
            "message": "No content found with id: 1001"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &add_args("read", "user", "acc-1"));
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(
        stderr.contains("Error: Failed to add restriction to page 1001: Resource not found"),
        "{stderr}"
    );
    server.verify().await;
}
