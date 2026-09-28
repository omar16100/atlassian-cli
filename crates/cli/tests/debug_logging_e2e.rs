//! `--debug` traces the requests a command makes, on stderr.
//!
//! It used to print nothing a failing request could be traced with: the filter
//! named a target that matched nothing, the flag was rejected after a
//! subcommand, and no response status or error body was ever logged. These run
//! the built binary against a mock Jira, because what reaches stderr and what
//! stays off stdout is the whole point.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");
const TOKEN: &str = "fake-token-value-that-must-not-leak";

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
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", TOKEN)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

async fn missing_issue_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/PROJ-404"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "errorMessages": ["Issue does not exist or you do not have permission to see it."]
        })))
        .mount(&server)
        .await;
    server
}

/// The reported case: a failing request, and `--debug` after the subcommand.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn debug_traces_a_failing_request_on_stderr() {
    let server = missing_issue_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["jira", "issue", "get", "PROJ-404", "--debug"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(stderr.contains("Sending request"), "{stderr}");
    assert!(stderr.contains("/rest/api/3/issue/PROJ-404"), "{stderr}");
    assert!(stderr.contains("status=404"), "{stderr}");
    assert!(stderr.contains("elapsed_ms="), "{stderr}");
    // A 404's body used to be dropped; now it is in the trace.
    assert!(stderr.contains("Issue does not exist"), "{stderr}");
    assert!(
        !stderr.contains(TOKEN),
        "the token must never be logged: {stderr}"
    );
    assert!(out.stdout.is_empty(), "logs stay off stdout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_debug_there_is_no_trace() {
    let server = missing_issue_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["jira", "issue", "get", "PROJ-404"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!stderr.contains("Sending request"), "{stderr}");
    assert!(!stderr.contains("status=404"), "{stderr}");
}

/// With `-f json`, the trace goes to stderr and stdout is still one JSON
/// document a script can parse.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn debug_leaves_json_output_parseable() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accountId": "acc-1",
            "displayName": "Dev User",
            "emailAddress": "dev@example.com"
        })))
        .mount(&server)
        .await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["auth", "whoami", "-f", "json", "--debug"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "{stderr}");
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}"));
    assert_eq!(parsed["account_id"], "acc-1");
    assert!(stderr.contains("status=200"), "{stderr}");
}
