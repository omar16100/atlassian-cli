//! The three `export` commands, end to end against a mock Jira and Confluence.
//!
//! `jira bulk export`, `jira audit export` and `confluence bulk export` each
//! declared a `format: String` argument of their own. That reused the id of the
//! global `--format` (`OutputFormat`), and clap panicked on every run of all
//! three ("Mismatch between definition and access of `format`") before a single
//! request was sent, whether `--format` was passed or not. The file format is
//! now `--export-format`; without it a global `--format json|csv` still picks
//! the file format, so the documented `--format json` examples keep working.
//!
//! These run the built binary, because the panic happened while turning the
//! parsed arguments into `Cli`, which no handler-level test reaches.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;
use wiremock::matchers::{body_partial_json, method, path as path_matcher, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

const JIRA_CSV_HEADER: &str = "key,summary,status,assignee,reporter,created";
const AUDIT_CSV_HEADER: &str = "id,summary,category,object_type,object_name,author,created";
const CONFLUENCE_CSV_HEADER: &str = "id,title,type,space";

/// Write a config pointing at `base_url`. HTTP is allowed because `ApiClient`
/// exempts localhost from its HTTPS requirement.
fn write_config(dir: &Path, base_url: &str) -> PathBuf {
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

fn run(config: &Path, args: &[&str]) -> Output {
    let dir = config.parent().unwrap_or_else(|| Path::new("."));
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        // Keep the CLI away from the developer's real configuration.
        .env("HOME", dir)
        .env("ATLASSIAN_CLI_CONFIG_DIR", dir)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("ATLASSIAN_API_TOKEN")
        .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
        .env_remove("BITBUCKET_TOKEN")
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

/// Fail with everything the CLI printed, so a panic is visible in the report.
fn assert_success(out: &Output) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "exit {:?}\nstdout: {stdout}\nstderr: {stderr}",
        out.status.code()
    );
    assert!(!stderr.contains("panicked"), "the CLI panicked: {stderr}");
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("export file {} not written: {e}", path.display()))
}

fn json_array_len(text: &str) -> usize {
    let value: serde_json::Value =
        serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"));
    value.as_array().expect("a JSON array").len()
}

async fn jira_search_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .and(body_partial_json(
            serde_json::json!({ "jql": "project = DEV" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issues": [
                {
                    "key": "DEV-1",
                    "fields": {
                        "summary": "First issue",
                        "status": { "name": "To Do" },
                        "assignee": { "displayName": "Ada" },
                        "reporter": { "displayName": "Grace" },
                        "created": "2026-09-01T10:00:00.000+0000"
                    }
                },
                {
                    "key": "DEV-2",
                    "fields": {
                        "summary": "Second issue",
                        "status": { "name": "Done" },
                        "created": "2026-09-02T10:00:00.000+0000"
                    }
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;
    server
}

async fn jira_audit_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/auditing/record"))
        .and(query_param("from", "2026-09-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "records": [{
                "id": 42,
                "summary": "User created",
                "category": "user management",
                "objectItem": { "typeName": "USER", "name": "ada" },
                "authorKey": "admin",
                "created": "2026-09-01T09:00:00.000+0000"
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    server
}

async fn confluence_search_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/rest/api/content/search"))
        .and(query_param("cql", "space = DEV"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{
                "content": {
                    "id": "12345",
                    "title": "Runbook",
                    "type": "page",
                    "space": { "key": "DEV" }
                }
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    server
}

/// The README example up to this fix (the output path aside): the exact command
/// line that panicked. Scripts written from it must keep working.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jira_bulk_export_with_the_documented_format_flag_writes_json() {
    let server = jira_search_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("issues.json");

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "export",
            "--jql",
            "project = DEV",
            "--output",
            file.to_str().unwrap(),
            "--format",
            "json",
        ],
    );

    assert_success(&out);
    assert_eq!(json_array_len(&read(&file)), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jira_bulk_export_defaults_to_json() {
    let server = jira_search_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("issues.json");

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "export",
            "--jql",
            "project = DEV",
            "--output",
            file.to_str().unwrap(),
        ],
    );

    assert_success(&out);
    let text = read(&file);
    assert_eq!(json_array_len(&text), 2);
    assert!(text.contains("DEV-1"), "{text}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Exported 2 issues"), "{stdout}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jira_bulk_export_writes_csv_with_export_format() {
    let server = jira_search_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("issues.csv");

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "export",
            "--jql",
            "project = DEV",
            "--output",
            file.to_str().unwrap(),
            "--export-format",
            "csv",
        ],
    );

    assert_success(&out);
    let text = read(&file);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], JIRA_CSV_HEADER);
    assert!(
        lines[1].starts_with("DEV-1,First issue,To Do,Ada,Grace,"),
        "{text}"
    );
    assert_eq!(lines.len(), 3, "{text}");
}

/// Without `--export-format`, `--format csv` has to write CSV: writing JSON
/// into a file the user named `issues.csv` would be the worst of both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jira_bulk_export_follows_a_global_csv_format() {
    let server = jira_search_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("issues.csv");

    let out = run(
        &config,
        &[
            "jira",
            "bulk",
            "export",
            "--jql",
            "project = DEV",
            "--output",
            file.to_str().unwrap(),
            "-f",
            "csv",
        ],
    );

    assert_success(&out);
    assert_eq!(read(&file).lines().next(), Some(JIRA_CSV_HEADER));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jira_audit_export_writes_csv_with_export_format() {
    let server = jira_audit_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("audit.csv");

    let out = run(
        &config,
        &[
            "jira",
            "audit",
            "export",
            "--from",
            "2026-09-01",
            "--output",
            file.to_str().unwrap(),
            "--export-format",
            "CSV",
        ],
    );

    assert_success(&out);
    let text = read(&file);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], AUDIT_CSV_HEADER);
    assert!(
        lines[1].starts_with("42,User created,user management,USER,ada,admin,"),
        "{text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jira_audit_export_defaults_to_json() {
    let server = jira_audit_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("audit.json");

    let out = run(
        &config,
        &[
            "jira",
            "audit",
            "export",
            "--from",
            "2026-09-01",
            "--output",
            file.to_str().unwrap(),
        ],
    );

    assert_success(&out);
    assert_eq!(json_array_len(&read(&file)), 1);
}

/// The Confluence README example up to this fix, and the form
/// `docs/examples/confluence/backup-space.sh` used.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn confluence_bulk_export_with_the_documented_format_flag_writes_json() {
    let server = confluence_search_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("backup.json");

    let out = run(
        &config,
        &[
            "confluence",
            "bulk",
            "export",
            "--cql",
            "space = DEV",
            "--output",
            file.to_str().unwrap(),
            "--format",
            "json",
        ],
    );

    assert_success(&out);
    assert_eq!(json_array_len(&read(&file)), 1);
}

/// The two flags are independent: `--export-format` picks the file, the global
/// `--format` still shapes the summary on stdout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn confluence_bulk_export_keeps_file_and_stdout_formats_apart() {
    let server = confluence_search_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("pages.csv");

    let out = run(
        &config,
        &[
            "confluence",
            "bulk",
            "export",
            "--cql",
            "space = DEV",
            "--output",
            file.to_str().unwrap(),
            "--export-format",
            "csv",
            "--format",
            "json",
        ],
    );

    assert_success(&out);
    let text = read(&file);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, [CONFLUENCE_CSV_HEADER, "12345,Runbook,page,DEV"]);

    // stdout carries a progress line, then the JSON summary.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let summary = &stdout[stdout.find('{').expect("a JSON summary on stdout")..];
    let summary: serde_json::Value = serde_json::from_str(summary)
        .unwrap_or_else(|e| panic!("summary not JSON ({e}): {stdout}"));
    assert_eq!(summary["success"], true, "{stdout}");
    assert!(
        summary["message"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Exported 1 pages"),
        "{stdout}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_export_format_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    // Nothing listens here: a usage error must fail before any request.
    let config = write_config(dir.path(), "http://127.0.0.1:1");
    let file = dir.path().join("pages.xml");

    let out = run(
        &config,
        &[
            "confluence",
            "bulk",
            "export",
            "--cql",
            "space = DEV",
            "--output",
            file.to_str().unwrap(),
            "--export-format",
            "xml",
        ],
    );

    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("possible values: json, csv"), "{stderr}");
    assert!(!file.exists());
}
