//! End-to-end tests for `jira issue comments` and `jira issue transitions`,
//! driving the real binary against a mock Jira.
//!
//! #100 was a wrong URL. A test that asks `ApiClient` to fetch a path it was
//! handed proves nothing about the path the *command* builds, so these spawn the
//! CLI and assert on which endpoint the mock actually received.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{body_json_string, method, path as path_matcher};
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
        // Keep the CLI away from the developer's real configuration: `--config`
        // moves only the config file, so without this the credential lookup
        // still reaches $HOME.
        .env("HOME", config.parent().unwrap_or_else(|| Path::new(".")))
        .env(
            "ATLASSIAN_CLI_CONFIG_DIR",
            config.parent().unwrap_or_else(|| Path::new(".")),
        )
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("ATLASSIAN_API_TOKEN")
        .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
        .env_remove("BITBUCKET_TOKEN")
        .args(args)
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .output()
        .expect("failed to run the CLI")
}

/// Regression for #100: the command used to PUT `/rest/api/3/comment/{id}`,
/// which Jira Cloud has no route for, so every update 404'd.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comment_update_targets_the_issue_scoped_endpoint() {
    let server = MockServer::start().await;

    // The route that does not exist. Nothing may reach it.
    Mock::given(method("PUT"))
        .and(path_matcher("/rest/api/3/comment/10100"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path_matcher("/rest/api/3/issue/TEST-1/comment/10100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "10100"})))
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
            "comments",
            "update",
            "TEST-1",
            "10100",
            "--body",
            "Updated text",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comment_delete_targets_the_issue_scoped_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path_matcher("/rest/api/3/comment/10100"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&server)
        .await;

    Mock::given(method("DELETE"))
        .and(path_matcher("/rest/api/3/issue/TEST-1/comment/10100"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "comments", "delete", "TEST-1", "10100"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The body is markdown converted to ADF, so an update must not send raw text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comment_update_sends_adf() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path_matcher("/rest/api/3/issue/TEST-1/comment/10100"))
        .and(body_json_string(
            serde_json::json!({
                "body": {
                    "type": "doc",
                    "version": 1,
                    "content": [{
                        "type": "paragraph",
                        "content": [{"type": "text", "text": "Plain text"}]
                    }]
                }
            })
            .to_string(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "10100"})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "comments",
            "update",
            "TEST-1",
            "10100",
            "--body",
            "Plain text",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// #101: the transitions available on an issue, discoverable rather than guessed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transitions_are_listed_with_their_target_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/TEST-1/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transitions": [
                {"id": "11", "name": "To Do", "to": {"name": "To Do"}},
                {"id": "21", "name": "In Progress", "to": {"name": "In Progress"}},
                // Older instances omit `to`; it must not abort the parse.
                {"id": "31", "name": "Done"}
            ]
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "transitions", "TEST-1", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert_eq!(rows[1]["id"], "21");
    assert_eq!(rows[1]["to"], "In Progress");
    assert_eq!(rows[2]["to"], "", "a missing `to` renders empty, not null");

    // The scripting case the issue asked for: ids, one per line.
    let quiet = run(
        &config,
        &[
            "jira",
            "issue",
            "transitions",
            "TEST-1",
            "--format",
            "quiet",
        ],
    );
    assert!(quiet.status.success());
    assert_eq!(
        String::from_utf8_lossy(&quiet.stdout).trim(),
        "11\n21\n31",
        "quiet mode should emit transition ids for scripting"
    );
}
