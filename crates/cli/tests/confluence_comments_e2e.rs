//! End-to-end tests for `confluence page comments` and `add-comment` (#122).
//!
//! The reported bug was invisible at the transport level: the command fetched a
//! real endpoint successfully and returned an empty list, because it only ever
//! asked for footer comments. What matters is therefore *which* endpoints the
//! command calls and what it does with the answers, so these drive the built
//! binary and assert on the requests the mock actually received.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{body_partial_json, method, path as path_matcher, query_param};
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
        // Keep the CLI away from the developer's real configuration: `--config`
        // moves only the config file, so without this the credential lookup
        // still reaches $HOME.
        .env("HOME", config.parent().unwrap_or_else(|| Path::new(".")))
        .env(
            "ATLASSIAN_CLI_CONFIG_DIR",
            config.parent().unwrap_or_else(|| Path::new(".")),
        )
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("ATLASSIAN_API_TOKEN")
        .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
        .env_remove("BITBUCKET_TOKEN")
        .args(args)
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .output()
        .expect("failed to run the CLI")
}

fn comment(id: &str, title: &str, body: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "title": title,
        "version": {"createdAt": "2026-08-21T10:00:00.000Z"},
        "body": {"storage": {"value": format!("<p>{body}</p>")}}
    })
}

/// The reported bug: a page whose comments are all inline listed as empty.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inline_comments_are_listed_alongside_footer_comments() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/footer-comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [comment("1", "Re: page", "A footer comment")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/inline-comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                comment("2", "Re: selection", "An inline comment"),
                comment("3", "Re: selection", "Another inline comment")
            ]
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "confluence",
            "page",
            "comments",
            "12345",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 3, "both collections should be listed: {rows:?}");
    assert_eq!(rows[0]["kind"], "footer");
    assert_eq!(rows[1]["kind"], "inline");
    assert_eq!(rows[2]["id"], "3");
    // A thread root has no parent, and the key is omitted rather than null.
    assert!(rows[0].get("parent").is_none());
}

/// Confluence v2 paginates with an opaque cursor. Ignoring it capped every
/// listing at the default page size, which is how a page with 84 comments
/// reported 25.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comment_pagination_is_followed_to_the_end() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/footer-comments"))
        .and(query_param("cursor", "page2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [comment("2", "Second", "Second page")]
        })))
        .mount(&server)
        .await;
    // Registered second so the cursor-bearing mock wins; this one answers the
    // first request and hands back the cursor.
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/footer-comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [comment("1", "First", "First page")],
            "_links": {"next": "/wiki/api/v2/pages/12345/footer-comments?body-format=storage&cursor=page2"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/inline-comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "confluence",
            "page",
            "comments",
            "12345",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        rows.as_array().unwrap().len(),
        2,
        "both pages should be fetched: {rows:?}"
    );
}

/// `--replies` walks each thread and links children to their root.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replies_are_fetched_and_linked_to_their_parent() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/footer-comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [comment("10", "Root", "Question?")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/footer-comments/10/children"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [comment("11", "Re: Root", "Answer.")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/inline-comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "confluence",
            "page",
            "comments",
            "12345",
            "--replies",
            "--format",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["id"], "11");
    assert_eq!(rows[1]["parent"], "10", "a reply must name its thread root");
}

/// Without `--replies`, the per-thread requests must not happen at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replies_are_not_fetched_by_default() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/footer-comments/10/children"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/footer-comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [comment("10", "Root", "Question?")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/inline-comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(&config, &["confluence", "page", "comments", "12345"]);
    assert!(out.status.success());
}

/// A reply goes to the collection its parent lives in: Confluence will not
/// accept a footer reply to an inline thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reply_posts_to_its_parents_collection() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_matcher("/wiki/api/v2/inline-comments"))
        .and(body_partial_json(serde_json::json!({
            "pageId": "12345",
            "parentCommentId": "98765"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"id": "99"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/wiki/api/v2/footer-comments"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "confluence",
            "page",
            "add-comment",
            "12345",
            "Agreed",
            "--parent",
            "98765",
            "--kind",
            "inline",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A plain comment still posts to footer-comments with no parent, unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_top_level_comment_is_unchanged() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_matcher("/wiki/api/v2/footer-comments"))
        .and(body_partial_json(serde_json::json!({
            "pageId": "12345",
            "status": "current"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"id": "99"})))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &["confluence", "page", "add-comment", "12345", "Looks good"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Storage format is XHTML, so a comment containing markup characters has to be
/// escaped or Confluence rejects or mangles it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comment_text_is_escaped_for_storage_format() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_matcher("/wiki/api/v2/footer-comments"))
        .and(body_partial_json(serde_json::json!({
            "body": {
                "representation": "storage",
                "value": "<p>a &lt; b &amp;&amp; c &gt; d</p>"
            }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"id": "99"})))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        &[
            "confluence",
            "page",
            "add-comment",
            "12345",
            "a < b && c > d",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The upload URL is built by hand from `base_url()`, which keeps its trailing
/// slash, so it used to produce `host//wiki/...`. wiremock matches the path as
/// sent, so a doubled slash simply fails to match and this test catches it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attachment_upload_does_not_double_the_slash() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_matcher(
            "/wiki/rest/api/content/12345/child/attachment",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{"id": "att1", "title": "note.txt"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    let file = dir.path().join("note.txt");
    std::fs::write(&file, b"hello").unwrap();

    let out = run(
        &config,
        &[
            "confluence",
            "attachment",
            "upload",
            "12345",
            "--file",
            file.to_str().unwrap(),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
