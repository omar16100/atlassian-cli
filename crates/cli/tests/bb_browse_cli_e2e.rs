//! `bb commit browse` through the built binary, for the parts that send no
//! request.
//!
//! The Bitbucket base URL is a constant in the binary, so these tests cannot
//! point the binary to a mock server. The request and content behaviour is
//! tested with `wiremock` in `crates/cli/src/commands/bitbucket/source.rs`.
//! This file covers the argument rules, the help text, and that a refused
//! path leaves stdout empty.

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

fn write_config(dir: &Path) -> std::path::PathBuf {
    let config_path = dir.join("config.yaml");
    std::fs::write(
        &config_path,
        "default_profile: local\n\
         profiles:\n\
         \x20 local:\n\
         \x20   email: dev@example.com\n\
         \x20   base_url: https://example.atlassian.net\n",
    )
    .unwrap();
    config_path
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let config = write_config(dir);
    let mut cmd = Command::new(BIN);
    // Remove each credential variable of the developer, so that the fake token
    // below is the only one.
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy().to_uppercase();
        if ["ATLASSIAN_", "BITBUCKET_", "JIRA_", "CONFLUENCE_"]
            .iter()
            .any(|p| name.starts_with(p))
        {
            cmd.env_remove(&key);
        }
    }
    cmd.arg("--config")
        .arg(&config)
        .arg("--config-dir")
        .arg(dir)
        .env("HOME", dir)
        .env_remove("XDG_CONFIG_HOME")
        .env("ATLASSIAN_CLI_BITBUCKET_TOKEN_LOCAL", "fake-token")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

#[test]
fn help_says_that_a_file_path_prints_the_content() {
    let dir = TempDir::new().unwrap();
    let out = run(dir.path(), &["bb", "commit", "browse", "--help"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("print the content of a file"),
        "the summary must name the file case: {stdout}"
    );
    assert!(
        stdout.contains("bb commit browse web_app --commit main --path src/main.rs"),
        "an example of a file read is missing: {stdout}"
    );
    assert!(stdout.contains("--output"), "{stdout}");
    assert!(stdout.contains("--force"), "{stdout}");
}

#[test]
fn commit_help_leads_to_browse_for_file_content() {
    let dir = TempDir::new().unwrap();
    let out = run(dir.path(), &["bb", "commit", "--help"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("print the content of a file"),
        "the command list must name the file case: {stdout}"
    );
}

#[test]
fn force_without_output_is_refused() {
    let dir = TempDir::new().unwrap();
    let out = run(
        dir.path(),
        &[
            "bb",
            "commit",
            "browse",
            "web_app",
            "--workspace",
            "myworkspace",
            "--commit",
            "main",
            "--path",
            "a.txt",
            "--force",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(stderr.contains("--output"), "got: {stderr}");
    assert!(out.stdout.is_empty());
}

// A dot segment is refused before a request is sent, so this run needs no
// server. Stdout must stay empty in a machine format: the error is on stderr.
#[test]
fn refused_path_exits_with_an_error_and_an_empty_stdout() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.bin");
    let out = run(
        dir.path(),
        &[
            "bb",
            "commit",
            "browse",
            "web_app",
            "--workspace",
            "myworkspace",
            "--commit",
            "main",
            "--path",
            "src/../../other_repo/src/main/secret.txt",
            "--output",
            target.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success());
    assert!(stderr.contains("Invalid path"), "got: {stderr}");
    assert!(out.stdout.is_empty(), "stdout: {:?}", out.stdout);
    assert!(!target.exists(), "no output file for a refused path");
}
