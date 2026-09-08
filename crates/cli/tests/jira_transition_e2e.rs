//! Transitions, end to end.
//!
//! The reported problem was that transitions are undiscoverable: a wrong name
//! produced `Transition 'Done' not found` and nothing else, so finding the real
//! name meant reading `/transitions` through a browser session. Transition names
//! are workflow-specific and are not status names -- one real workflow moves
//! Backlog to Analysis via "Start work", then to In Progress via "start
//! implementation", lowercase and mid-sentence. Nobody guesses that.
//!
//! These drive the built binary against a mock Jira, because the error text and
//! the success line are the product here.

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

fn transition(id: &str, name: &str, to: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "to": {"name": to, "statusCategory": {"name": "In Progress"}}
    })
}

/// The workflow from the report: names that look nothing like their targets.
async fn backlog_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/MLENG-1753/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transitions": [transition("11", "Start work", "Analysis")]
        })))
        .mount(&server)
        .await;
    server
}

/// The heart of the report: a wrong name must say what the right ones are.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_transition_lists_the_valid_ones() {
    let server = backlog_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "transition",
            "MLENG-1753",
            "--transition",
            "Done",
        ],
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(!out.status.success(), "a bad name must fail");
    assert!(
        combined.contains("Start work"),
        "the error must name what would have worked: {combined}"
    );
    assert!(
        combined.contains("Analysis"),
        "and where it leads: {combined}"
    );
    assert!(
        combined.contains("--to-status"),
        "and point at the alternative: {combined}"
    );
}

/// The success line used to echo the transition name, reporting
/// "Transitioned to: Completed work" for a move that lands in Done.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn success_reports_the_resulting_status_not_the_transition_name() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/MLENG-1753/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transitions": [transition("31", "Completed work", "Done")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue/MLENG-1753/transitions"))
        .respond_with(ResponseTemplate::new(204).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "transition",
            "MLENG-1753",
            "--transition",
            "Completed work",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("to: Done"),
        "must report the destination status: {stdout}"
    );
    assert!(
        !stdout.contains("to: Completed work"),
        "must not echo the transition name as the destination: {stdout}"
    );
}

/// Parity with `bulk transition`, on the command people run unrehearsed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dry_run_sends_nothing() {
    let server = backlog_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "transition",
            "MLENG-1753",
            "--transition",
            "Start work",
            "--dry-run",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("Analysis"),
        "shows the destination: {stdout}"
    );
    assert!(stdout.contains("nothing sent"), "{stdout}");

    let posts = server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.method == wiremock::http::Method::POST)
        .count();
    assert_eq!(posts, 0, "a dry run must not transition anything");
}

/// The 24-calls-to-8 problem: walk Backlog -> Analysis -> In Progress -> Done
/// without the caller naming a single workflow-specific transition.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn to_status_walks_the_workflow() {
    let server = MockServer::start().await;

    // The issue's status advances as the walk POSTs; wiremock serves mounted
    // responses in order, so each GET returns the next state.
    for (status, avail) in [
        ("Backlog", transition("11", "Start work", "Analysis")),
        (
            "Analysis",
            transition("21", "start implementation", "In Progress"),
        ),
        ("In Progress", transition("31", "Completed work", "Done")),
    ] {
        Mock::given(method("GET"))
            .and(path_matcher("/rest/api/3/issue/MLENG-1753"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "key": "MLENG-1753",
                "fields": {"status": {"name": status, "statusCategory": {"name": "To Do"}}}
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_matcher("/rest/api/3/issue/MLENG-1753/transitions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"transitions": [avail]})),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }

    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue/MLENG-1753/transitions"))
        .respond_with(ResponseTemplate::new(204).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "jira",
            "issue",
            "transition",
            "MLENG-1753",
            "--to-status",
            "Done",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    for step in ["Analysis", "In Progress", "Done"] {
        assert!(stdout.contains(step), "path should show {step}: {stdout}");
    }

    let posts = server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.method == wiremock::http::Method::POST)
        .count();
    assert_eq!(posts, 3, "one POST per hop");
}

/// So a script can tell whether a workflow-specific status counts as finished
/// without knowing the workflow.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn issue_get_exposes_the_status_category() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/MLENG-1753"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "MLENG-1753",
            "fields": {
                "summary": "Something",
                "status": {"name": "Analysis", "statusCategory": {"name": "In Progress"}}
            }
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["jira", "issue", "get", "MLENG-1753", "-f", "json"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");

    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["status"], serde_json::json!("Analysis"));
    assert_eq!(
        parsed["status_category"],
        serde_json::json!("In Progress"),
        "the category is what tells a script this is not done: {stdout}"
    );
}
