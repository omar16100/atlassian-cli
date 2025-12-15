use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cli_version() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("atlassian-cli"));
    // Check for semver pattern (0.x.y)
    assert!(stdout.contains("0.1."));
}

#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("jira"));
    assert!(stdout.contains("confluence"));
    assert!(stdout.contains("bitbucket"));
}

#[test]
fn test_jira_help() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "jira", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Jira commands"));
    assert!(stdout.contains("search"));
    assert!(stdout.contains("create"));
}

#[test]
fn test_confluence_help() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "confluence", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Confluence commands"));
}

#[test]
fn test_auth_help() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "auth", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Authentication commands"));
}

#[test]
fn test_output_format_flag() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "--output", "json", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_invalid_command() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "nonexistent"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unrecognized subcommand") || stderr.contains("error:"));
}

/// Regression test: Bitbucket-only profiles (no base_url) should not fail with "missing base_url" error.
/// This tests that profile resolution is properly split between Bitbucket and Jira/Confluence commands.
#[test]
fn test_bitbucket_only_profile_no_base_url_error() {
    // Create a temp config with a Bitbucket-only profile (no base_url)
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("config.yaml");

    let config_content = r#"
default_profile: bbonly
profiles:
  bbonly:
    email: test@example.com
    workspace: myworkspace
"#;

    std::fs::write(&config_path, config_content).expect("Failed to write config");

    // Run a bitbucket command with the custom config and a fake token via env var
    // The command will fail at the API call (no real credentials), but should NOT fail
    // at profile resolution with "missing base_url" error
    let output = Command::new("cargo")
        .args([
            "run", "--quiet", "--",
            "--config", config_path.to_str().unwrap(),
            "bitbucket", "repo", "list"
        ])
        .env("ATLASSIAN_CLI_BITBUCKET_TOKEN_BBONLY", "fake-token")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT fail with "missing base_url" - that was the bug
    assert!(
        !stderr.contains("missing a base_url"),
        "Bitbucket-only profile should not require base_url. Got: {stderr}"
    );

    // Expected: fails at API level (401 or connection error), not profile resolution
    // The command will fail, but for the right reason (no valid credentials/API error)
}

/// Test that Jira commands still require base_url (ensure we didn't break existing behavior).
#[test]
fn test_jira_still_requires_base_url() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("config.yaml");

    // Profile with only email (no base_url) - should fail for Jira
    let config_content = r#"
default_profile: nobaseurl
profiles:
  nobaseurl:
    email: test@example.com
"#;

    std::fs::write(&config_path, config_content).expect("Failed to write config");

    let output = Command::new("cargo")
        .args([
            "run", "--quiet", "--",
            "--config", config_path.to_str().unwrap(),
            "jira", "issue", "search"
        ])
        .env("ATLASSIAN_CLI_TOKEN_NOBASEURL", "fake-token")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail with "missing base_url" for Jira commands
    assert!(
        stderr.contains("missing a base_url") || stderr.contains("base-url"),
        "Jira commands should require base_url. Got: {stderr}"
    );
}
