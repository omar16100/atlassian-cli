//! A successful command writes nothing to stderr at the default log level.
//!
//! The default level was `info`, and almost every command logs an `info` line
//! that repeats its stdout message ("Issue created successfully"). Callers that
//! capture output with `2>&1` got that line in the middle of the JSON, and the
//! parse failed. These run the built binary against a mock Jira, because the
//! bytes on each stream are the product here.

mod common;

use std::process::{Command, Output};

use common::Sandbox;
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A sandbox with one profile that points at `base_url`.
fn sandbox_for(base_url: &str) -> Sandbox {
    let sandbox = Sandbox::new();
    std::fs::create_dir_all(sandbox.config_dir()).unwrap();
    std::fs::write(
        sandbox.config_dir().join("config.yaml"),
        format!(
            "default_profile: local\nprofiles:\n  local:\n    email: dev@example.com\n    base_url: {base_url}\n"
        ),
    )
    .unwrap();
    sandbox
}

fn cli(sandbox: &Sandbox) -> Command {
    let mut cmd = sandbox.cli();
    cmd.env_remove("RUST_LOG")
        .env("NO_COLOR", "1")
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token");
    cmd
}

fn run(sandbox: &Sandbox, args: &[&str]) -> Output {
    cli(sandbox)
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

/// Without a token in the environment, for the `auth` commands.
fn plain(sandbox: &Sandbox, args: &[&str]) -> Output {
    sandbox
        .cli()
        .env_remove("RUST_LOG")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

async fn create_issue_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(serde_json::json!({"id": "10001", "key": "PROJ-1"})),
        )
        .mount(&server)
        .await;
    server
}

const CREATE: [&str; 11] = [
    "jira",
    "issue",
    "create",
    "--project",
    "PROJ",
    "--issue-type",
    "Task",
    "--summary",
    "Example",
    "-f",
    "json",
];

/// The reported case: `2>&1` on a JSON command must give one JSON document.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_successful_json_command_writes_nothing_to_stderr() {
    let server = create_issue_server().await;
    let sandbox = sandbox_for(&server.uri());

    let out = run(&sandbox, &CREATE);

    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        out.stderr.is_empty(),
        "stderr must be empty on success: {}",
        text(&out.stderr)
    );
    // What a caller sees with `2>&1`: the two streams together.
    let combined = [out.stdout.as_slice(), out.stderr.as_slice()].concat();
    let value: serde_json::Value = serde_json::from_slice(&combined)
        .unwrap_or_else(|e| panic!("not one JSON document ({e}): {}", text(&combined)));
    assert_eq!(value["id"], "PROJ-1");
}

/// "No issues found" was the other line that the scan found in JSON streams.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_search_writes_nothing_to_stderr() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"issues": [], "isLast": true})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"accountId": "acc-1"})),
        )
        .mount(&server)
        .await;
    let sandbox = sandbox_for(&server.uri());

    let out = run(
        &sandbox,
        &[
            "jira",
            "issue",
            "search",
            "--jql",
            "project = PROJ",
            "-f",
            "json",
        ],
    );

    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value, serde_json::json!([]));
}

/// The lines are still available: `RUST_LOG=info` and `--debug` show them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rust_log_info_and_debug_still_show_the_success_line() {
    let server = create_issue_server().await;
    let sandbox = sandbox_for(&server.uri());

    let with_env = cli(&sandbox)
        .env("RUST_LOG", "info")
        .args(CREATE)
        .output()
        .unwrap();
    assert!(
        text(&with_env.stderr).contains("Issue created successfully"),
        "{}",
        text(&with_env.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&with_env.stdout).expect("stdout stays JSON");

    let with_flag = cli(&sandbox).args(CREATE).arg("--debug").output().unwrap();
    assert!(
        text(&with_flag.stderr).contains("Issue created successfully"),
        "{}",
        text(&with_flag.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&with_flag.stdout).expect("stdout stays JSON");
}

/// `auth login` and `auth logout` reported their result only in a log line, so
/// a quiet stderr would have left them with no output at all.
#[test]
fn login_and_logout_confirm_on_stdout() {
    let sandbox = Sandbox::new();

    let login = plain(
        &sandbox,
        &[
            "auth",
            "login",
            "--profile",
            "local",
            "--base-url",
            "https://example.atlassian.net",
            "--email",
            "dev@example.com",
            "--token",
            "fake-token",
        ],
    );
    assert!(login.status.success(), "{}", text(&login.stderr));
    assert!(login.stderr.is_empty(), "{}", text(&login.stderr));
    let stdout = text(&login.stdout);
    assert!(stdout.contains("local"), "{stdout}");
    assert!(stdout.contains("https://example.atlassian.net"), "{stdout}");
    assert!(
        !stdout.contains("fake-token"),
        "the token must not be printed"
    );

    let logout = plain(
        &sandbox,
        &["auth", "logout", "--profile", "local", "-f", "json"],
    );
    assert!(logout.status.success(), "{}", text(&logout.stderr));
    assert!(logout.stderr.is_empty(), "{}", text(&logout.stderr));
    let value: serde_json::Value = serde_json::from_slice(&logout.stdout).unwrap();
    assert_eq!(value["success"], true);
    assert_eq!(value["id"], "local");
}

/// In a machine format, the stdout of a login is the result object only. A
/// `/wiki` base URL printed a note before it, and a URL with a password in it
/// would have been printed back.
#[test]
fn login_in_json_prints_one_document_and_no_url_credentials() {
    let sandbox = Sandbox::new();

    let login = plain(
        &sandbox,
        &[
            "auth",
            "login",
            "--profile",
            "local",
            "--base-url",
            "https://someone:hunter2@example.atlassian.net/wiki",
            "--email",
            "dev@example.com",
            "--token",
            "fake-token",
            "-f",
            "json",
        ],
    );

    assert!(login.status.success(), "{}", text(&login.stderr));
    assert!(login.stderr.is_empty(), "{}", text(&login.stderr));
    let value: serde_json::Value = serde_json::from_slice(&login.stdout)
        .unwrap_or_else(|e| panic!("not one JSON document ({e}): {}", text(&login.stdout)));
    assert_eq!(value["success"], true);
    assert_eq!(value["id"], "local");
    let stdout = text(&login.stdout);
    assert!(!stdout.contains("hunter2"), "{stdout}");
    assert!(!stdout.contains("fake-token"), "{stdout}");
}

/// Logout reported success when it could not read the credentials file, so the
/// token could still be on disk.
#[test]
fn logout_fails_when_the_credentials_file_cannot_be_read() {
    let sandbox = Sandbox::new();
    std::fs::create_dir_all(sandbox.config_dir()).unwrap();
    std::fs::write(
        sandbox.config_dir().join("config.yaml"),
        "default_profile: local\nprofiles:\n  local:\n    email: dev@example.com\n    base_url: https://example.atlassian.net\n",
    )
    .unwrap();
    std::fs::write(sandbox.config_dir().join("credentials.enc"), "not json").unwrap();

    let logout = plain(
        &sandbox,
        &["auth", "logout", "--profile", "local", "-f", "json"],
    );

    assert!(!logout.status.success(), "{}", text(&logout.stdout));
    assert!(logout.stdout.is_empty(), "{}", text(&logout.stdout));
    assert!(
        text(&logout.stderr).contains("encrypted credentials file"),
        "{}",
        text(&logout.stderr)
    );
}

/// The hint for an empty profile list was a log line. It is on stdout now for
/// the formats that people read, and a machine format gets an empty list.
#[test]
fn an_empty_profile_list_keeps_its_hint() {
    let sandbox = Sandbox::new();

    let table = plain(&sandbox, &["auth", "list"]);
    assert!(table.status.success(), "{}", text(&table.stderr));
    assert!(table.stderr.is_empty(), "{}", text(&table.stderr));
    assert!(
        text(&table.stdout).contains("auth login"),
        "{}",
        text(&table.stdout)
    );

    let json = plain(&sandbox, &["auth", "list", "-f", "json"]);
    assert!(json.stderr.is_empty(), "{}", text(&json.stderr));
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value, serde_json::json!([]));
}

/// A cancelled delete said so only in a log line.
#[test]
fn a_cancelled_project_delete_says_so_on_stdout() {
    use std::io::Write;
    use std::process::Stdio;

    let sandbox = Sandbox::new();
    std::fs::create_dir_all(sandbox.config_dir()).unwrap();
    std::fs::write(
        sandbox.config_dir().join("config.yaml"),
        "default_profile: local\nprofiles:\n  local:\n    email: dev@example.com\n    workspace: myworkspace\n",
    )
    .unwrap();

    let mut child = cli(&sandbox)
        .args(["bitbucket", "project", "delete", "PROJ"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let out = child.wait_with_output().unwrap();

    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains("was not deleted"),
        "{}",
        text(&out.stdout)
    );
}
