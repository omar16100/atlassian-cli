//! `api` passthrough wiring, per product.
//!
//! The passthrough existed only under `jira`, so every field the Bitbucket and
//! Confluence DTOs drop was unreachable. Wiring it elsewhere looks like a
//! copy of two lines from `jira/mod.rs`, and for Confluence it is. Bitbucket is
//! not: `bitbucket::execute` requires a workspace -- from `--workspace`, the git
//! remote, or the profile -- before it builds its context, and only `Whoami`
//! escapes that. Wired at the bottom of the match the way Jira's is,
//! `bb api /2.0/user` would fail with "Workspace required" for anyone outside a
//! Bitbucket checkout, on the one command whose purpose is reaching endpoints
//! the typed commands cannot.
//!
//! These drive the built binary with `--dry-run`, which resolves the request and
//! sends nothing, so they assert the wiring without needing a mock for
//! api.bitbucket.org (whose base URL is a constant, not a profile field).

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

/// A profile with an email and no workspace, and crucially no `base_url`
/// pointing anywhere real. Nothing is sent, so none of it needs to resolve.
fn write_config(dir: &Path, extra: &str) -> std::path::PathBuf {
    let config_path = dir.join("config.yaml");
    let yaml = format!(
        "default_profile: local\n\
         profiles:\n\
         \x20 local:\n\
         \x20   email: dev@example.com\n\
         \x20   base_url: https://example.atlassian.net\n\
         {extra}"
    );
    std::fs::write(&config_path, yaml).unwrap();
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
        .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
        .env("ATLASSIAN_CLI_BITBUCKET_TOKEN_LOCAL", "fake-token")
        // Run somewhere with no Bitbucket git remote, so nothing can supply a
        // workspace behind the test's back and make it pass for the wrong
        // reason.
        .current_dir(std::env::temp_dir())
        .args(args)
        .output()
        .expect("failed to run the CLI")
}

/// The regression this file exists for.
#[test]
fn bb_api_resolves_without_a_workspace() {
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), "");

    let out = run(&config, &["bb", "api", "/2.0/user", "--dry-run"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "bb api must not require a workspace.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("Workspace required"),
        "the workspace gate must not apply to the passthrough: {stderr}"
    );
    assert!(
        stdout.contains("https://api.bitbucket.org/2.0/user"),
        "expected the resolved Bitbucket URL, got: {stdout}"
    );
}

/// The other half of the guarantee: exempting the passthrough must not exempt
/// anything else. If this ever passes, the early return is catching too much.
#[test]
fn typed_bitbucket_commands_still_require_a_workspace() {
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), "");

    let out = run(&config, &["bb", "repo", "list"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(!out.status.success(), "expected the workspace gate to fire");
    assert!(
        combined.contains("Workspace required"),
        "expected the workspace error, got: {combined}"
    );
}

#[test]
fn bb_api_honours_the_method_and_query_flags() {
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), "");

    let out = run(
        &config,
        &[
            "bb",
            "api",
            "/2.0/repositories/ws/repo/pullrequests",
            "--query",
            "state=MERGED",
            "--dry-run",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("state=MERGED"), "query missing: {stdout}");
}

#[test]
fn confluence_api_resolves_against_the_profile_site() {
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), "");

    let out = run(
        &config,
        &["confluence", "api", "/wiki/api/v2/pages", "--dry-run"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("https://example.atlassian.net/wiki/api/v2/pages"),
        "expected the profile's site, got: {stdout}"
    );
}

/// Bitbucket's passthrough must target api.bitbucket.org, not the profile's
/// Atlassian site. Getting this wrong would send Bitbucket credentials to the
/// Jira host.
#[test]
fn bb_api_does_not_target_the_atlassian_site() {
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), "");

    let out = run(&config, &["bb", "api", "/2.0/user", "--dry-run"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        !stdout.contains("example.atlassian.net"),
        "Bitbucket passthrough must not resolve against the Jira site: {stdout}"
    );
}
