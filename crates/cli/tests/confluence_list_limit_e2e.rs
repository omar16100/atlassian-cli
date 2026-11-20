//! End-to-end tests for `--limit` on `confluence space list`, `page list` and
//! `blog list`. They run the built binary against a mock Confluence.
//!
//! The three commands sent `--limit` as the `limit` query parameter of one
//! request. The v2 endpoints accept 250 at most, so `--limit 300` failed with a
//! 400, and a result larger than one server page was cut without a warning.
//! `--limit` is now a total: the page size is clamped to 250 and the command
//! follows `_links.next`.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{method, path as path_matcher, query_param, query_param_is_missing};
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
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

fn item(n: usize) -> serde_json::Value {
    serde_json::json!({
        "id": n.to_string(),
        "key": format!("S{n}"),
        "name": format!("Item {n}"),
        "title": format!("Item {n}"),
        "type": "global",
        "status": "current",
    })
}

fn items(range: std::ops::RangeInclusive<usize>) -> Vec<serde_json::Value> {
    range.map(item).collect()
}

fn rows(out: &std::process::Output) -> Vec<serde_json::Value> {
    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Mount a first page of 250 items (asked for with `limit=250`) and a second
/// page of 1 item behind the `next` link. Each must be requested `calls` times.
async fn mount_251(server: &MockServer, collection: &str, calls: u64) {
    Mock::given(method("GET"))
        .and(path_matcher(format!("/wiki/api/v2/{collection}")))
        .and(query_param("cursor", "page-2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "results": items(251..=251) })),
        )
        .with_priority(1)
        .expect(calls)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher(format!("/wiki/api/v2/{collection}")))
        .and(query_param("limit", "250"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": items(1..=250),
            "_links": {
                "next": format!("/wiki/api/v2/{collection}?limit=250&cursor=page-2"),
                "base": format!("{}/wiki", server.uri()),
            }
        })))
        .expect(calls)
        .mount(server)
        .await;
}

/// The reported failure: a limit above 250 was sent as it was and the server
/// refused it. The page size is now 250 and the second request follows `next`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn space_list_above_the_maximum_clamps_and_follows_next() {
    let server = MockServer::start().await;
    mount_251(&server, "spaces", 1).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(
        &config,
        &[
            "confluence",
            "space",
            "list",
            "--limit",
            "251",
            "-f",
            "json",
        ],
    );

    let rows = rows(&out);
    assert_eq!(rows.len(), 251);
    assert_eq!(rows[250]["id"], "251");
    assert_eq!(rows[250]["key"], "S251");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("warning"),
        "a complete result must not warn: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    server.verify().await;
}

/// A small limit is one request with that page size, and the `type` filter is
/// kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn space_list_small_limit_is_one_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/spaces"))
        .and(query_param("limit", "5"))
        .and(query_param("type", "global"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "results": items(1..=3) })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(
        &config,
        &[
            "confluence",
            "space",
            "list",
            "--limit",
            "5",
            "--space-type",
            "global",
            "-f",
            "json",
        ],
    );

    assert_eq!(rows(&out).len(), 3);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    server.verify().await;
}

/// The limit is a total. When the server has more, the result is cut to the
/// limit and stderr says so. Stdout stays valid JSON.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn space_list_warns_when_the_limit_cuts_the_result() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/spaces"))
        .and(query_param("limit", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": items(1..=2),
            "_links": { "next": "/wiki/api/v2/spaces?limit=2&cursor=page-2" }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(
        &config,
        &["confluence", "space", "list", "--limit", "2", "-f", "json"],
    );

    assert_eq!(rows(&out).len(), 2);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("warning: showing 2 spaces"), "{stderr}");
    assert!(stderr.contains("--limit 0"), "{stderr}");
    server.verify().await;
}

/// `--limit 0` asks for everything: page size 250, all pages.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn space_list_limit_zero_returns_all() {
    let server = MockServer::start().await;
    mount_251(&server, "spaces", 1).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(
        &config,
        &["confluence", "space", "list", "--limit", "0", "-f", "json"],
    );

    assert_eq!(rows(&out).len(), 251);
    assert!(!String::from_utf8_lossy(&out.stderr).contains("warning"));
    server.verify().await;
}

/// With no flag the command keeps its one page of 25.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn space_list_without_a_limit_asks_for_one_page_of_25() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/spaces"))
        .and(query_param("limit", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": items(1..=25),
            "_links": { "next": "/wiki/api/v2/spaces?limit=25&cursor=page-2" }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(&config, &["confluence", "space", "list", "-f", "json"]);

    assert_eq!(rows(&out).len(), 25);
    assert!(String::from_utf8_lossy(&out.stderr).contains("warning: showing 25 spaces"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn page_list_above_the_maximum_clamps_and_follows_next() {
    let server = MockServer::start().await;
    mount_251(&server, "pages", 1).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(
        &config,
        &["confluence", "page", "list", "--limit", "300", "-f", "json"],
    );

    let rows = rows(&out);
    assert_eq!(rows.len(), 251);
    assert_eq!(rows[250]["title"], "Item 251");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blog_list_above_the_maximum_clamps_and_follows_next() {
    let server = MockServer::start().await;
    mount_251(&server, "blogposts", 1).await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let out = run(
        &config,
        &["confluence", "blog", "list", "--limit", "300", "-f", "json"],
    );

    let rows = rows(&out);
    assert_eq!(rows.len(), 251);
    assert_eq!(rows[250]["title"], "Item 251");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    server.verify().await;
}
