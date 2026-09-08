//! `auth whoami` must honour `-f`.
//!
//! Every field went out through `println!`, so `-f json` returned the text
//! form and a script could not parse it. The fix routes the machine formats
//! through the renderer while leaving the human output alone, so these assert
//! both halves: JSON parses, and the table form still reads as labelled lines
//! rather than a one-row grid nobody asked for.

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

async fn jira_identity_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accountId": "acc-123",
            "displayName": "Dev Example",
            "emailAddress": "dev@example.com",
            "active": true
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn whoami_json_is_parseable() {
    let server = jira_identity_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["auth", "whoami", "-f", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");

    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("-f json did not produce JSON ({e}): {stdout}"));

    assert_eq!(parsed["account_id"], serde_json::json!("acc-123"));
    assert_eq!(parsed["display_name"], serde_json::json!("Dev Example"));
    assert_eq!(parsed["product"], serde_json::json!("Jira"));
    assert_eq!(parsed["profile"], serde_json::json!("local"));
    assert_eq!(parsed["active"], serde_json::json!(true));
}

/// Confluence does not report `active`. Emitting `false` would claim the
/// account is disabled when the API simply never said either way.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreported_active_flag_is_omitted_not_false() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/rest/api/user/current"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accountId": "acc-9",
            "publicName": "Wiki User"
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &format!("{}/wiki", server.uri()));

    let out = run(&config, &["auth", "whoami", "-f", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");

    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["product"], serde_json::json!("Confluence"));
    assert!(
        parsed.get("active").is_none(),
        "an unreported flag must be absent, not false: {stdout}"
    );
}

/// The human output is what people already read; only the machine formats
/// changed. A one-row table here would be a regression, not a fix.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_table_form_is_still_labelled_lines() {
    let server = jira_identity_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["auth", "whoami"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("Profile: local"), "{stdout}");
    assert!(stdout.contains("Product: Jira"), "{stdout}");
    assert!(stdout.contains("Account ID: acc-123"), "{stdout}");
    // `tabled` draws with Unicode box characters (Style::rounded), so checking
    // for ASCII `|`/`+` would have passed even if the output became a table.
    for border in ['│', '─', '╭', '┼', '╰'] {
        assert!(
            !stdout.contains(border),
            "the readable form must not have become a table: {stdout}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn whoami_yaml_is_parseable() {
    let server = jira_identity_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["auth", "whoami", "-f", "yaml"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");

    let parsed: serde_yaml::Value = serde_yaml::from_str(&stdout)
        .unwrap_or_else(|e| panic!("-f yaml did not produce YAML ({e}): {stdout}"));
    assert_eq!(parsed["account_id"].as_str(), Some("acc-123"));
}

/// `-f quiet` and `-f csv` are consumed by line-oriented scripts. The old code
/// printed labelled lines for every format; only the structured formats should
/// have changed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn line_oriented_formats_keep_their_text_form() {
    let server = jira_identity_server().await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    for format in ["quiet", "csv"] {
        let out = run(&config, &["auth", "whoami", "-f", format]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{format}: {stdout}");
        assert!(
            stdout.contains("Account ID: acc-123"),
            "-f {format} must keep the labelled lines, got: {stdout}"
        );
        assert!(
            !stdout.trim_start().starts_with('{'),
            "-f {format} must not emit a JSON object: {stdout}"
        );
    }
}
