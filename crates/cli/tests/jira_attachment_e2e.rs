//! End-to-end tests for `jira attachment`, driving the real binary against a
//! mock Jira.
//!
//! The other integration tests exercise `ApiClient` directly, which proves the
//! transport but not the command: dispatch, metadata lookup, filename
//! sanitization, clobber refusal and stdout streaming all live above it. These
//! tests spawn the compiled CLI with a temp config so the whole path runs.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{method, path as path_matcher, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nFAKEIMAGEDATA";

/// Write a config pointing at `base_url`. HTTP is allowed because `ApiClient`
/// exempts localhost from its HTTPS requirement.
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

fn run(config: &Path, cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        .args(args)
        .current_dir(cwd)
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .output()
        .expect("failed to run the CLI")
}

/// Two attachments share a filename and one carries a traversal attempt, so a
/// single run covers sanitization and de-duplication.
async fn mock_issue_with_attachments(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/TEST-1"))
        .and(query_param("fields", "attachment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "TEST-1",
            "fields": {"attachment": [
                {"id": 10001, "filename": "diagram.png", "mimeType": "image/png", "size": PNG.len()},
                {"id": "10002", "filename": "../../etc/evil.txt", "mimeType": "text/plain", "size": 5},
                {"id": "10003", "filename": "diagram.png", "mimeType": "image/png", "size": 14}
            ]}
        })))
        .mount(server)
        .await;
}

async fn mock_attachment(server: &MockServer, id: &str, filename: &str, body: &'static [u8]) {
    Mock::given(method("GET"))
        .and(path_matcher(format!("/rest/api/3/attachment/{id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": id, "filename": filename, "mimeType": "image/png", "size": body.len()
        })))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_matcher(format!("/rest/api/3/attachment/content/{id}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_without_output_uses_the_server_filename() {
    let server = MockServer::start().await;
    mock_attachment(&server, "10001", "diagram.png", PNG).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["jira", "attachment", "download", "10001"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(dir.path().join("diagram.png")).unwrap(), PNG);
}

/// The flagship agent use case: the byte stream must survive a pipe untouched,
/// including with `--debug`, which is what the stderr logging fix is for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_to_stdout_is_byte_exact() {
    let server = MockServer::start().await;
    mock_attachment(&server, "10001", "diagram.png", PNG).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["jira", "attachment", "download", "10001", "--output", "-"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, PNG);

    let debug = run(
        &config,
        dir.path(),
        &[
            "--debug",
            "jira",
            "attachment",
            "download",
            "10001",
            "--output",
            "-",
        ],
    );
    assert!(debug.status.success());
    assert_eq!(
        debug.stdout, PNG,
        "logs must not reach stdout, even at debug level"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_refuses_to_clobber_without_force() {
    let server = MockServer::start().await;
    mock_attachment(&server, "10001", "diagram.png", PNG).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    std::fs::write(dir.path().join("out.png"), b"existing").unwrap();

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "attachment",
            "download",
            "10001",
            "--output",
            "out.png",
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
    assert_eq!(
        std::fs::read(dir.path().join("out.png")).unwrap(),
        b"existing"
    );

    let forced = run(
        &config,
        dir.path(),
        &[
            "jira",
            "attachment",
            "download",
            "10001",
            "--output",
            "out.png",
            "--force",
        ],
    );
    assert!(forced.status.success());
    assert_eq!(std::fs::read(dir.path().join("out.png")).unwrap(), PNG);
}

/// A traversal attempt in a server-supplied filename must land inside `--dir`,
/// and a duplicate filename must not clobber its predecessor.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_download_sanitizes_filenames_and_deduplicates() {
    let server = MockServer::start().await;
    mock_issue_with_attachments(&server).await;
    for (id, body) in [
        ("10001", &b"\x89PNG\r\n\x1a\nFAKEIMAGEDATA"[..]),
        ("10002", &b"EVIL!"[..]),
        ("10003", &b"SECOND-DIAGRAM"[..]),
    ] {
        Mock::given(method("GET"))
            .and(path_matcher(format!("/rest/api/3/attachment/content/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
            .mount(&server)
            .await;
    }

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "attachment",
            "download",
            "--issue",
            "TEST-1",
            "--dir",
            "att",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let att = dir.path().join("att");
    assert_eq!(std::fs::read(att.join("diagram.png")).unwrap(), PNG);
    assert_eq!(std::fs::read(att.join("evil.txt")).unwrap(), b"EVIL!");
    assert_eq!(
        std::fs::read(att.join("10003-diagram.png")).unwrap(),
        b"SECOND-DIAGRAM"
    );
    // Nothing may be written outside the target directory.
    assert!(!dir.path().join("etc").exists());
    assert_eq!(std::fs::read_dir(&att).unwrap().count(), 3);
}

/// A server-supplied id that could alter a URL or a path fails only its own row.
///
/// The two attachments deliberately share a filename: that is what makes the id
/// reach the on-disk name, via the de-duplication prefix. Without validation the
/// second write lands at `att/../../owned-dup.png`, outside `--dir`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_download_rejects_a_hostile_attachment_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/TEST-1"))
        .and(query_param("fields", "attachment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "TEST-1",
            "fields": {"attachment": [
                {"id": "10001", "filename": "dup.png", "size": PNG.len()},
                {"id": "../../owned", "filename": "dup.png", "size": 5}
            ]}
        })))
        .mount(&server)
        .await;
    // Deliberately permissive: any content request at all is answered, so the
    // only thing that can stop the hostile row is our own validation.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG.to_vec()))
        .mount(&server)
        .await;

    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().join("cwd");
    std::fs::create_dir(&cwd).unwrap();
    let config = write_config(workspace.path(), &server.uri());

    let out = run(
        &config,
        &cwd,
        &[
            "jira",
            "attachment",
            "download",
            "--issue",
            "TEST-1",
            "--dir",
            "att",
        ],
    );

    // Partial failure: the good file lands, the run exits non-zero.
    assert!(!out.status.success());
    assert_eq!(std::fs::read(cwd.join("att/dup.png")).unwrap(), PNG);
    assert_eq!(std::fs::read_dir(cwd.join("att")).unwrap().count(), 1);
    // `att/../../owned-dup.png` resolves into the workspace root.
    assert!(!workspace.path().join("owned-dup.png").exists());
    assert!(!cwd.join("owned-dup.png").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_renders_rows_in_json() {
    let server = MockServer::start().await;
    mock_issue_with_attachments(&server).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["jira", "attachment", "list", "TEST-1", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 3);
    // Numeric and string ids both normalize to strings.
    assert_eq!(rows[0]["id"], "10001");
    assert_eq!(rows[1]["id"], "10002");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_sends_multipart_and_reports_ids() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue/TEST-1/attachments"))
        .and(wiremock::matchers::header("X-Atlassian-Token", "no-check"))
        .and(wiremock::matchers::body_string_contains("name=\"file\""))
        .and(wiremock::matchers::body_string_contains(
            "filename=\"note.txt\"",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "20001", "filename": "note.txt", "size": 5}
        ])))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    std::fs::write(dir.path().join("note.txt"), b"hello").unwrap();

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "attachment",
            "upload",
            "TEST-1",
            "--file",
            "note.txt",
            "--format",
            "quiet",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "20001");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_requires_force_and_sends_no_request_without_it() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path_matcher("/rest/api/3/attachment/10001"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let unforced = run(
        &config,
        dir.path(),
        &["jira", "attachment", "delete", "10001"],
    );
    assert!(unforced.status.success());
    assert!(String::from_utf8_lossy(&unforced.stdout).contains("--force"));

    let forced = run(
        &config,
        dir.path(),
        &["jira", "attachment", "delete", "10001", "--force"],
    );
    assert!(
        forced.status.success(),
        "{}",
        String::from_utf8_lossy(&forced.stderr)
    );
}

/// A hostile id is rejected before any request leaves the process.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_attachment_id_is_rejected_before_any_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["jira", "attachment", "get", "../../admin"],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Invalid attachment id"));
}
