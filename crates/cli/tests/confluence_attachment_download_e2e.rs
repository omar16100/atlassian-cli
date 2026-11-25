//! End-to-end tests for `confluence attachment download` (#204).
//!
//! The command gained `--output -` (stdout), a default file name, `--force`,
//! and `--page <ID> --dir <DIR>` to fetch every attachment of a page. Before,
//! `--output -` created a file literally named `-`. These drive the built
//! binary against a mock server and assert on the bytes written.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use wiremock::matchers::{header_exists, method, path as path_matcher, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake-png-bytes\x00\xff";
const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>";

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

/// Run the CLI with `cwd` as working directory, isolated from the real config.
fn run(config: &Path, cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .arg("--config")
        .arg(config)
        .current_dir(cwd)
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

fn detail(id: &str, title: &str, link: &str) -> serde_json::Value {
    serde_json::json!({"id": id, "title": title, "downloadLink": link})
}

/// Mount the metadata endpoint for one attachment and its bytes.
async fn mount_attachment(server: &MockServer, id: &str, title: &str, file: &str, bytes: &[u8]) {
    let link = format!("/download/attachments/12345/{file}?version=1&api=v2");
    Mock::given(method("GET"))
        .and(path_matcher(format!("/wiki/api/v2/attachments/{id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(detail(id, title, &link)))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher(format!(
            "/wiki/download/attachments/12345/{file}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.to_vec()))
        .mount(server)
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn output_file_receives_the_exact_bytes() {
    let server = MockServer::start().await;
    mount_attachment(&server, "att1", "logo.png", "logo.png", PNG).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "confluence",
            "attachment",
            "download",
            "att1",
            "--output",
            "out.png",
        ],
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(dir.path().join("out.png")).unwrap(), PNG);
}

/// The reported bug: `--output -` wrote a file named `-` instead of stdout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn output_dash_streams_only_the_bytes_to_stdout() {
    let server = MockServer::start().await;
    mount_attachment(&server, "att1", "logo.png", "logo.png", PNG).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "confluence",
            "attachment",
            "download",
            "att1",
            "--output",
            "-",
        ],
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Nothing but the bytes: no success line mixed into the stream.
    assert_eq!(out.stdout, PNG);
    assert!(
        !dir.path().join("-").exists(),
        "a file named '-' was created"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_output_the_title_is_used_in_the_current_directory() {
    let server = MockServer::start().await;
    mount_attachment(&server, "att1", "diagram.svg", "diagram.svg", SVG).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["confluence", "attachment", "download", "att1"],
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(dir.path().join("diagram.svg")).unwrap(), SVG);
}

/// A title chosen by the uploader must not write outside the working directory.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_path_in_the_title_does_not_escape_the_directory() {
    let server = MockServer::start().await;
    mount_attachment(&server, "att1", "../../escaped.png", "escaped.png", PNG).await;
    let root = TempDir::new().unwrap();
    let work = root.path().join("a").join("b");
    std::fs::create_dir_all(&work).unwrap();
    let config = write_config(root.path(), &server.uri());

    let out = run(
        &config,
        &work,
        &["confluence", "attachment", "download", "att1"],
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(work.join("escaped.png").exists());
    assert!(!root.path().join("escaped.png").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_existing_file_needs_force() {
    let server = MockServer::start().await;
    mount_attachment(&server, "att1", "logo.png", "logo.png", PNG).await;
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());
    std::fs::write(dir.path().join("out.png"), b"keep me").unwrap();

    let out = run(
        &config,
        dir.path(),
        &[
            "confluence",
            "attachment",
            "download",
            "att1",
            "--output",
            "out.png",
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));
    assert_eq!(
        std::fs::read(dir.path().join("out.png")).unwrap(),
        b"keep me"
    );

    let out = run(
        &config,
        dir.path(),
        &[
            "confluence",
            "attachment",
            "download",
            "att1",
            "--output",
            "out.png",
            "--force",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(dir.path().join("out.png")).unwrap(), PNG);
}

/// Confluence answers the download link with a redirect to a media host. The
/// bytes must arrive, and the profile's credentials must not go to that host.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redirect_to_the_media_host_is_followed_without_credentials() {
    let media = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/file/logo.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG.to_vec()))
        .mount(&media)
        .await;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/attachments/att1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(detail(
            "att1",
            "logo.png",
            "/download/attachments/12345/logo.png?version=1&api=v2",
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/download/attachments/12345/logo.png"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "location",
            format!("{}/file/logo.png", media.uri()).as_str(),
        ))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "confluence",
            "attachment",
            "download",
            "att1",
            "--output",
            "-",
        ],
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, PNG);
    let media_requests: Vec<Request> = media.received_requests().await.unwrap();
    assert_eq!(media_requests.len(), 1);
    assert!(
        !media_requests[0].headers.contains_key("authorization"),
        "credentials leaked to the media host"
    );
}

/// `--page` reads every page of the listing, keeps duplicate titles apart and
/// writes into `--dir`, creating it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn page_download_fetches_all_listing_pages_into_the_directory() {
    let server = MockServer::start().await;

    let first = serde_json::json!({
        "results": [
            detail("att1", "logo.png", "/download/attachments/12345/logo.png?version=1&api=v2"),
            detail("att2", "Logo.png", "/download/attachments/12345/logo2.png?version=1&api=v2"),
        ],
        "_links": {"next": "/wiki/api/v2/pages/12345/attachments?cursor=page2"}
    });
    let second = serde_json::json!({
        "results": [
            detail("att3", "diagram.svg", "/download/attachments/12345/diagram.svg?version=1&api=v2"),
        ],
        "_links": {}
    });
    // The cursor request is mounted first so it wins over the plain listing.
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/attachments"))
        .and(query_param("cursor", "page2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(second))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/attachments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(first))
        .expect(1)
        .mount(&server)
        .await;
    for (file, bytes) in [
        ("logo.png", PNG),
        ("logo2.png", b"second".as_slice()),
        ("diagram.svg", SVG),
    ] {
        Mock::given(method("GET"))
            .and(path_matcher(format!(
                "/wiki/download/attachments/12345/{file}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.to_vec()))
            .mount(&server)
            .await;
    }

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &[
            "confluence",
            "attachment",
            "download",
            "--page",
            "12345",
            "--dir",
            "out/files",
        ],
    );

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let target = dir.path().join("out").join("files");
    assert_eq!(std::fs::read(target.join("logo.png")).unwrap(), PNG);
    assert_eq!(
        std::fs::read(target.join("Logo-att2.png")).unwrap(),
        b"second"
    );
    assert_eq!(std::fs::read(target.join("diagram.svg")).unwrap(), SVG);
}

/// One failed file does not stop the others, but the run exits non-zero.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn page_download_reports_a_failed_file_and_continues() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/api/v2/pages/12345/attachments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                detail("att1", "gone.png", "/download/attachments/12345/gone.png?version=1&api=v2"),
                detail("att2", "ok.svg", "/download/attachments/12345/ok.svg?version=1&api=v2"),
            ],
            "_links": {}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/download/attachments/12345/gone.png"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_matcher("/wiki/download/attachments/12345/ok.svg"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(SVG.to_vec()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), &server.uri());

    let out = run(
        &config,
        dir.path(),
        &["confluence", "attachment", "download", "--page", "12345"],
    );

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("att1"), "stderr: {stderr}");
    assert_eq!(std::fs::read(dir.path().join("ok.svg")).unwrap(), SVG);
}

#[test]
fn output_and_page_cannot_be_combined() {
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), "http://127.0.0.1:9");

    let out = run(
        &config,
        dir.path(),
        &[
            "confluence",
            "attachment",
            "download",
            "--page",
            "12345",
            "--output",
            "x.png",
        ],
    );

    assert_eq!(out.status.code(), Some(2), "clap usage error expected");
}
