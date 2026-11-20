//! End-to-end tests for `confluence page remove-restriction`. They run the
//! built binary against a mock Confluence.
//!
//! The expected requests come from the Confluence Cloud REST API v1
//! specification,
//! <https://dac-static.atlassian.com/cloud/confluence/swagger.v3.json>:
//! `DELETE /wiki/rest/api/content/{id}/restriction/byOperation/{operationKey}/user`
//! with the `accountId` query parameter, and
//! `DELETE .../byOperation/{operationKey}/byGroupId/{groupId}`.
//! They are not verified against a live Confluence site.
//!
//! The command sent `DELETE /wiki/rest/api/content/{id}/restriction` with
//! `operation` and `user.identifier` query parameters. The specification has
//! no such parameters on that endpoint, and says that it removes all
//! restrictions of the content. Each test here mounts that endpoint with an
//! expected count of 0.

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

fn remove_args<'a>(operation: &'a str, subject_type: &'a str, subject_id: &'a str) -> Vec<&'a str> {
    vec![
        "confluence",
        "page",
        "remove-restriction",
        "1001",
        "--operation",
        operation,
        "--subject-type",
        subject_type,
        "--subject-id",
        subject_id,
    ]
}

/// The DELETE that removes all restrictions of the content. No test in this
/// file must reach it.
async fn forbid_delete_all(server: &MockServer) {
    Mock::given(method("DELETE"))
        .and(path_matcher("/wiki/rest/api/content/1001/restriction"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(server)
        .await;
}

#[tokio::test]
async fn remove_restriction_for_a_user_uses_the_by_operation_endpoint() {
    let server = MockServer::start().await;
    forbid_delete_all(&server).await;
    Mock::given(method("DELETE"))
        .and(path_matcher(
            "/wiki/rest/api/content/1001/restriction/byOperation/read/user",
        ))
        // An account ID can have a colon. The query value must arrive unchanged.
        .and(query_param("accountId", "557058:acc-1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &remove_args("read", "user", "557058:acc-1"));

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout)
        .contains("Removed read restriction for 557058:acc-1 from page 1001"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    server.verify().await;
}

#[tokio::test]
async fn remove_restriction_for_a_group_uses_the_group_id_endpoint() {
    let server = MockServer::start().await;
    forbid_delete_all(&server).await;
    Mock::given(method("DELETE"))
        .and(path_matcher(
            "/wiki/rest/api/content/1001/restriction/byOperation/update/byGroupId/grp-1",
        ))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &remove_args("update", "group", "grp-1"));

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    server.verify().await;
}

/// A value outside the documented set is refused before a request is sent.
/// The operation goes into the request path, so this is also a path guard.
#[tokio::test]
async fn an_unknown_operation_or_subject_type_sends_no_request() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    // The values are case-sensitive, so `Read` is refused too.
    for bad_operation in ["delete", "Read", "read/../update", "read?x=1"] {
        let out = run(&config, &remove_args(bad_operation, "user", "acc-1"));
        assert!(!out.status.success(), "{bad_operation}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("--operation"), "{stderr}");
        assert!(
            stderr.contains("[possible values: read, update]"),
            "{stderr}"
        );
    }

    for bad_subject in ["team", "User"] {
        let out = run(&config, &remove_args("read", bad_subject, "acc-1"));
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
        .args(["confluence", "page", "remove-restriction", "--help"])
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
    assert!(stdout.contains("user account ID or group ID"), "{stdout}");
}

/// An empty or blank `--subject-id` names no subject. No request is sent, for
/// a user and for a group.
#[tokio::test]
async fn an_empty_subject_id_sends_no_request() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    for subject_type in ["user", "group"] {
        for blank in ["", "   "] {
            let out = run(&config, &remove_args("read", subject_type, blank));
            assert!(!out.status.success(), "{subject_type} {blank:?}");
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                stderr.contains("--subject-id must not be empty"),
                "{stderr}"
            );
        }
    }

    assert!(server.received_requests().await.unwrap().is_empty());
}

/// The value that is checked is the value that is sent: spaces at the two ends
/// are removed.
#[tokio::test]
async fn the_subject_id_is_sent_trimmed() {
    let server = MockServer::start().await;
    forbid_delete_all(&server).await;
    Mock::given(method("DELETE"))
        .and(path_matcher(
            "/wiki/rest/api/content/1001/restriction/byOperation/read/user",
        ))
        .and(query_param("accountId", "acc-1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &remove_args("read", "user", "  acc-1 "));

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
    forbid_delete_all(&server).await;
    Mock::given(method("DELETE"))
        .and(path_matcher(
            "/wiki/rest/api/content/1001/restriction/byOperation/read/user",
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "statusCode": 404,
            "message": "No content found with id: 1001"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &remove_args("read", "user", "acc-1"));
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(
        stderr.contains("Error: Failed to remove restriction from page 1001: Resource not found"),
        "{stderr}"
    );
    server.verify().await;
}

/// A group ID is one path segment. A value with `/` is refused, so it cannot
/// address a different resource.
#[tokio::test]
async fn remove_restriction_refuses_a_group_id_with_a_slash() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &remove_args("read", "group", "a/b"));

    assert!(!out.status.success());
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A group ID with a query marker is encoded into the path segment. It must
/// not reach the endpoint of a different group.
#[tokio::test]
async fn remove_restriction_encodes_the_group_id() {
    let server = MockServer::start().await;
    forbid_delete_all(&server).await;
    Mock::given(method("DELETE"))
        .and(path_matcher(
            "/wiki/rest/api/content/1001/restriction/byOperation/read/byGroupId/grp-1%3Fx%3D1",
        ))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &remove_args("read", "group", "grp-1?x=1"));

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    server.verify().await;
}
