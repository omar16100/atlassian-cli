//! End-to-end tests for `jira api`, driving the real binary against a mock Jira.
//!
//! `crates/cli` has no lib target, so the command's internals are unreachable
//! from an integration test. Spawning the binary is the only way to cover
//! dispatch, the output contract and the safety gates together.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{body_string, header, method, path as path_matcher, query_param};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_is_the_default_method_and_json_is_pretty_printed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .and(header(
            "authorization",
            "Basic ZGV2QGV4YW1wbGUuY29tOmZha2UtdG9rZW4=",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"accountId": "abc", "displayName": "Dev"})),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, dir.path(), &["jira", "api", "/rest/api/3/myself"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"displayName\": \"Dev\""), "got {stdout}");
}

/// The reason `--query` exists: JQL is full of spaces and equals signs, which
/// users would otherwise have to percent-encode by hand.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn query_pairs_are_percent_encoded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/search/jql"))
        .and(query_param("jql", "project = TEST AND status = Open"))
        .and(query_param("maxResults", "5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"issues": []})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "api",
            "/rest/api/3/search/jql",
            "--query",
            "jql=project = TEST AND status = Open",
            "--query",
            "maxResults=5",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A body implies POST, goes on the wire verbatim, and gets a JSON content type.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn data_implies_post_and_is_sent_verbatim() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/issue"))
        .and(header("content-type", "application/json"))
        // Byte-for-byte, including the unsorted keys.
        .and(body_string(
            r#"{"fields":{"summary":"x","project":{"key":"T"}}}"#,
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"key": "T-1"})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "api",
            "/rest/api/3/issue",
            "-d",
            r#"{"fields":{"summary":"x","project":{"key":"T"}}}"#,
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("T-1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_headers_reach_the_wire() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/x"))
        .and(header("X-ExperimentalApi", "opt-in"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "api",
            "/rest/api/3/x",
            "-H",
            "X-ExperimentalApi: opt-in",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The whole point of a passthrough: show what the API actually said, rather
/// than replacing it with our own error text. Still exits non-zero.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_2xx_prints_the_api_error_body_and_exits_non_zero() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/NOPE-1"))
        .respond_with(ResponseTemplate::new(404).set_body_json(
            serde_json::json!({"errorMessages": ["Issue does not exist or you do not have permission to see it."]}),
        ))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["jira", "api", "/rest/api/3/issue/NOPE-1"],
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Issue does not exist"),
        "the API's own body must survive: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("HTTP 404"));
}

/// `request` coerces an empty 2xx body to JSON `null`; the raw path prints nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_204_prints_nothing_and_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path_matcher("/rest/api/3/issue/T-1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "api",
            "/rest/api/3/issue/T-1",
            "-X",
            "put",
            "-d",
            "{}",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "got {:?}", out.stdout);
}

/// This is what makes the passthrough answer issue #93 on its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_body_goes_to_output_byte_for_byte() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\n\xff\xfe\x00BINARY";
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/attachment/content/10001"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png.to_vec()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "api",
            "/rest/api/3/attachment/content/10001",
            "--output",
            "shot.png",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(dir.path().join("shot.png")).unwrap(), png);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_without_force_sends_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["jira", "api", "/rest/api/3/issue/T-1", "-X", "delete"],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dry_run_sends_nothing_and_never_prints_credentials() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "jira",
            "api",
            "/rest/api/3/issue/T-1",
            "-X",
            "delete",
            "--dry-run",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("DELETE"), "got {stdout}");
    assert!(stdout.contains("/rest/api/3/issue/T-1"), "got {stdout}");
    assert!(
        !stdout.to_lowercase().contains("authorization"),
        "credentials must not appear: {stdout}"
    );
    assert!(!stdout.contains("fake-token"), "got {stdout}");
}

/// The same-origin check is what makes an arbitrary-path command safe.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_host_path_is_rejected_before_sending() {
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
        &["jira", "api", "https://evil.example.com/steal"],
    );
    assert!(!out.status.success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overriding_the_authorization_header_is_refused() {
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
        &[
            "jira",
            "api",
            "/rest/api/3/myself",
            "-H",
            "Authorization: Bearer stolen",
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Authorization"));
}

/// `--output` must behave like `jira attachment download`: no silent clobber.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn output_refuses_to_clobber_without_force() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    std::fs::write(dir.path().join("out.json"), b"existing").unwrap();

    let out = run(
        &config,
        dir.path(),
        &["jira", "api", "/rest/api/3/myself", "--output", "out.json"],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
    assert_eq!(
        std::fs::read(dir.path().join("out.json")).unwrap(),
        b"existing"
    );

    let forced = run(
        &config,
        dir.path(),
        &[
            "jira",
            "api",
            "/rest/api/3/myself",
            "--output",
            "out.json",
            "--force",
        ],
    );
    assert!(forced.status.success());
    assert!(std::fs::read(dir.path().join("out.json"))
        .unwrap()
        .starts_with(b"{"));
}

/// A same-origin endpoint must not be able to bounce a credentialed, body-carrying
/// request to another host. The 3xx comes back instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_origin_redirect_is_returned_not_followed() {
    let evil = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("pwned"))
        .expect(0)
        .mount(&evil)
        .await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/rest/api/3/bounce"))
        .respond_with(
            ResponseTemplate::new(307)
                .insert_header("location", format!("{}/steal", evil.uri()).as_str()),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["jira", "api", "/rest/api/3/bounce", "-d", "{}"],
    );
    // 307 is not 2xx, so this exits 1 and the user sees the redirect.
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("HTTP 307"));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("pwned"));
}

/// Regression: the origin check compared scheme and host but not port, so any
/// other port on the same host could receive the profile's credentials.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_different_port_on_the_same_host_is_rejected() {
    let victim = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("secrets"))
        .expect(0)
        .mount(&victim)
        .await;

    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let target = format!("{}/steal", victim.uri());
    let out = run(&config, dir.path(), &["jira", "api", &target]);
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("secrets"));
}
