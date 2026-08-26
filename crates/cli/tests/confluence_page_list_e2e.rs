//! End-to-end tests for `confluence page list`, driving the real binary against
//! a mock Confluence.
//!
//! `confluence_integration.rs` asks `ApiClient` to fetch a URL the test itself
//! wrote, which is self-consistent whatever the command does: it passed both
//! before and after `--space` was fixed, and merely documented the broken
//! parameter for anyone copying it. The bug being guarded here is precisely
//! that the *command* built the wrong query, so the command is what runs.
//!
//! `/wiki/api/v2/pages` filters on a numeric `space-id` and has no `space-key`
//! parameter. Unknown parameters are dropped rather than rejected, so passing
//! `space-key` returned the whole site: two different keys gave byte-identical
//! output, which looks like a space listing and is not.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{method, path as path_matcher, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

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

fn run(config: &Path, args: &[&str]) -> std::process::Output {
    let dir = config.parent().unwrap_or_else(|| Path::new("."));
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        // Keep the CLI away from the developer's real configuration: `--config`
        // moves only the config file, so without this the credential lookup
        // still reaches $HOME.
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

/// `keys=DOCS` resolves to space id 786433, the only shape the pages endpoint
/// accepts.
async fn mock_space_lookup(server: &MockServer, key: &str, id: &str) {
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/spaces"))
        .and(query_param("keys", key))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{ "id": id, "key": key, "name": "Documentation" }]
        })))
        .expect(1)
        .mount(server)
        .await;
}

async fn mock_pages_for_space(server: &MockServer, space_id: &str, title: &str) {
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages"))
        .and(query_param("space-id", space_id))
        // The parameter that did nothing. Asserting it is absent is the point of
        // the test: sending both would pass a `space-id` check while leaving the
        // bug in place.
        .and(query_param_is_missing("space-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{
                "id": "123456",
                "title": title,
                "status": "current",
                "spaceId": space_id,
            }]
        })))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn space_filter_resolves_the_key_and_queries_by_id() {
    let server = MockServer::start().await;
    mock_space_lookup(&server, "DOCS", "786433").await;
    mock_pages_for_space(&server, "786433", "Onboarding").await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["confluence", "page", "list", "--space", "DOCS"]);

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Onboarding"),
        "expected the space's page in stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    // Both mocks carry `.expect(1)`, verified on drop.
}

/// The regression itself: two keys must not return the same pages. Each mock
/// answers for one space id only, so a request carrying the wrong id, or no id,
/// goes unmatched and the command fails.
#[tokio::test]
async fn two_different_keys_return_their_own_pages() {
    for (key, id, title) in [
        ("DOCS", "786433", "Onboarding"),
        ("OPS", "917505", "Runbook"),
    ] {
        let server = MockServer::start().await;
        mock_space_lookup(&server, key, id).await;
        mock_pages_for_space(&server, id, title).await;

        let dir = TempDir::new().unwrap();
        let config = write_config(dir.path(), &server.uri());

        let out = run(&config, &["confluence", "page", "list", "--space", key]);

        assert!(
            out.status.success(),
            "listing {key} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(title),
            "{key} should list {title}: {stdout}"
        );
    }
}

/// An unknown key is an error, not a site-wide listing that looks plausible.
#[tokio::test]
async fn an_unknown_space_key_fails_without_listing_anything() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/spaces"))
        .and(query_param("keys", "NOPE"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "results": [] })),
        )
        .expect(1)
        .mount(&server)
        .await;

    // Never reached. Mounted so a regression that skips resolution and lists the
    // whole site fails here rather than passing quietly.
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{ "id": "1", "title": "Some other space's page", "status": "current" }]
        })))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["confluence", "page", "list", "--space", "NOPE"]);

    assert!(!out.status.success(), "an unknown key should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("NOPE"),
        "the error should name the key: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("Some other space"),
        "no pages should have been listed"
    );
}

/// Resolution is only for `--space`. An unfiltered listing must not pay for a
/// lookup it does not need.
#[tokio::test]
async fn listing_without_a_space_does_not_look_up_any_space() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/spaces"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "results": [] })),
        )
        .expect(0)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages"))
        .and(query_param_is_missing("space-id"))
        .and(query_param_is_missing("space-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{ "id": "1", "title": "Anything", "status": "current" }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["confluence", "page", "list"]);

    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
