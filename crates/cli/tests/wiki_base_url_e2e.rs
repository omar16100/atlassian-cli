//! A profile whose `base_url` ends in `/wiki` must work, for every product.
//!
//! `ApiClient` appends request paths to the base rather than replacing the
//! base's path, and every Confluence command spells `/wiki` itself while every
//! Jira command spells `/rest/api/3`. So a base written as
//! `https://site.atlassian.net/wiki` asked for `/wiki/wiki/api/v2/pages` and
//! `/wiki/rest/api/3/issue/KEY`, and both 404.
//!
//! Writing `/wiki` into the base is an easy mistake: it is how the Confluence
//! REST documentation spells the full URL. Before this suite there was no test
//! anywhere that used such a base, which is why the fault survived so long.
//!
//! Each test mounts the doubled path as well, expecting zero hits, so a
//! regression fails loudly here instead of turning into a 404 for a user.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{method, path as path_matcher, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

/// A profile pointed at `<base_url>`, written verbatim so the tests can choose
/// the shape under test.
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
        .env_remove("ATLASSIAN_API_TOKEN")
        .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
        .env_remove("BITBUCKET_TOKEN")
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

fn assert_ok(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The doubled path, mounted to catch a regression rather than let it 404.
async fn trap_doubled(server: &MockServer, doubled: &str) {
    Mock::given(method("GET"))
        .and(path_matcher(doubled.to_string()))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(server)
        .await;
}

#[tokio::test]
async fn confluence_page_list_resolves_once_under_a_wiki_base() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{ "id": "1", "title": "Onboarding", "status": "current" }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    trap_doubled(&server, "/wiki/wiki/api/v2/pages").await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &format!("{}/wiki", server.uri()));

    let out = run(&config, &["confluence", "page", "list"]);

    assert_ok(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Onboarding"));
}

/// The space filter takes two requests, so both must resolve correctly.
#[tokio::test]
async fn confluence_space_filter_resolves_once_under_a_wiki_base() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/spaces"))
        .and(query_param("keys", "DOCS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{ "id": "786433", "key": "DOCS", "name": "Docs" }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages"))
        .and(query_param("space-id", "786433"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{ "id": "1", "title": "Runbook", "status": "current" }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    trap_doubled(&server, "/wiki/wiki/api/v2/spaces").await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &format!("{}/wiki", server.uri()));

    let out = run(&config, &["confluence", "page", "list", "--space", "DOCS"]);

    assert_ok(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Runbook"));
}

/// Jira was broken by the same base, and more obviously: `/wiki` has no
/// business in a Jira path at all.
#[tokio::test]
async fn jira_is_not_prefixed_with_wiki_under_a_wiki_base() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "DEV-1",
            "fields": { "summary": "Fix the login error", "status": {"name": "Open"} }
        })))
        .expect(1)
        .mount(&server)
        .await;
    trap_doubled(&server, "/wiki/rest/api/3/issue/DEV-1").await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &format!("{}/wiki", server.uri()));

    let out = run(&config, &["jira", "issue", "get", "DEV-1"]);

    assert_ok(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Fix the login error"));
}

/// The other half of the fix: the `/wiki` suffix is still what tells the CLI
/// this is a Confluence profile. Strip it everywhere and `auth whoami` would go
/// back to Jira's endpoint and 401, which is the bug PR #131 fixed.
#[tokio::test]
async fn a_wiki_base_is_still_recognised_as_confluence() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/rest/api/user/current"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accountId": "abc123",
            "displayName": "Ada Lovelace",
            "email": "ada@example.com"
        })))
        .expect(1)
        .mount(&server)
        .await;
    // Where it would go if the suffix were treated as a Jira profile.
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/myself"))
        .respond_with(ResponseTemplate::new(401))
        .expect(0)
        .mount(&server)
        .await;
    trap_doubled(&server, "/wiki/wiki/rest/api/user/current").await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &format!("{}/wiki", server.uri()));

    let out = run(&config, &["auth", "whoami"]);

    assert_ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Product: Confluence"), "{text}");
    assert!(text.contains("Ada Lovelace"), "{text}");
}

/// A plain site is untouched by any of this.
#[tokio::test]
async fn a_site_root_base_is_unaffected() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{ "id": "1", "title": "Unchanged", "status": "current" }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/rest/api/3/issue/DEV-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "DEV-1", "fields": { "summary": "Unchanged" }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    assert_ok(&run(&config, &["confluence", "page", "list"]));
    assert_ok(&run(&config, &["jira", "issue", "get", "DEV-1"]));
}
