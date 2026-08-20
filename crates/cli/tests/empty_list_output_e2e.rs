//! Empty list results must stay machine-readable (#110).
//!
//! ~60 list commands printed "No X found" on stdout when a query returned
//! nothing, in every format. Under `--format json` that is prose where a script
//! expects an array, so `| jq length` fails on exactly the case it most needs to
//! handle. These tests capture real stdout from the built binary, because that
//! is the only thing that proves what a caller receives.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

fn write_config(dir: &Path, base_url: &str) -> std::path::PathBuf {
    let config_path = dir.join("config.yaml");
    std::fs::write(
        &config_path,
        format!(
            "default_profile: local\nprofiles:\n  local:\n    email: dev@example.com\n    base_url: {base_url}\n    workspace: myteam\n"
        ),
    )
    .unwrap();
    config_path
}

fn run(config: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        .args(args)
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .output()
        .expect("failed to run the CLI")
}

// The case in the report was `bitbucket pr reviewers --format json`. Bitbucket
// commands build their client from BITBUCKET_API_URL rather than the profile's
// base_url, so they cannot be pointed at a mock; the same code path is covered
// below through Jira, and `reviewer_rows` has its own unit tests for the empty
// case in commands/bitbucket/pullrequests.rs.

/// Not one command: the same guarantee across products and formats.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_jira_search_is_an_empty_array_in_every_machine_format() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issues": [], "total": 0
        })))
        .mount(&server)
        .await;
    // `search` calls verify_auth when the result set is empty, to tell "no
    // matches" apart from "bad credentials".
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"accountId": "x"})),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let json = run(
        &config,
        &[
            "jira",
            "issue",
            "search",
            "--jql",
            "project = NONE",
            "--format",
            "json",
        ],
    );
    assert!(
        json.status.success(),
        "{}",
        String::from_utf8_lossy(&json.stderr)
    );
    let stdout = String::from_utf8_lossy(&json.stdout);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(stdout.trim()).unwrap(),
        serde_json::json!([]),
        "got: {stdout:?}"
    );

    // Quiet mode is for `for id in $(...)`: nothing to iterate, so no output.
    let quiet = run(
        &config,
        &[
            "jira",
            "issue",
            "search",
            "--jql",
            "project = NONE",
            "--format",
            "quiet",
        ],
    );
    assert!(quiet.status.success());
    assert!(
        quiet.stdout.is_empty(),
        "quiet mode should print nothing, got: {:?}",
        String::from_utf8_lossy(&quiet.stdout)
    );
}
