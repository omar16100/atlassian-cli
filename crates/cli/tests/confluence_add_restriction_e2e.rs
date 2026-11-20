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

    let bad_operation = run(&config, &add_args("delete", "user", "acc-1"));
    assert!(!bad_operation.status.success());
    let stderr = String::from_utf8_lossy(&bad_operation.stderr);
    assert!(
        stderr.contains("Invalid --operation 'delete'. Use read or update."),
        "{stderr}"
    );

    let bad_subject = run(&config, &add_args("read", "team", "acc-1"));
    assert!(!bad_subject.status.success());
    let stderr = String::from_utf8_lossy(&bad_subject.stderr);
    assert!(
        stderr.contains("Invalid --subject-type 'team'. Use user or group."),
        "{stderr}"
    );

    assert!(server.received_requests().await.unwrap().is_empty());
}
