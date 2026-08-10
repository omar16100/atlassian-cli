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
    // Check version matches Cargo.toml (works with any version)
    const EXPECTED_VERSION: &str = env!("CARGO_PKG_VERSION");
    assert!(
        stdout.contains(EXPECTED_VERSION),
        "CLI should output version {}, got: {}",
        EXPECTED_VERSION,
        stdout
    );
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
    // Verify alias is shown in help (visible_alias shows as [alias: bb])
    assert!(stdout.contains("bb") || stdout.contains("[alias"));
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
        .args(["run", "--quiet", "--", "--format", "json", "--help"])
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

#[test]
fn test_bb_alias_works() {
    // Test that 'bb' alias executes the same as 'bitbucket'
    let bb_output = Command::new("cargo")
        .args(["run", "--quiet", "--", "bb", "--help"])
        .output()
        .expect("Failed to execute bb alias");

    let bitbucket_output = Command::new("cargo")
        .args(["run", "--quiet", "--", "bitbucket", "--help"])
        .output()
        .expect("Failed to execute bitbucket command");

    assert!(bb_output.status.success());
    assert!(bitbucket_output.status.success());

    // Both should produce similar help output for bitbucket subcommands
    let bb_help = String::from_utf8_lossy(&bb_output.stdout);
    let bitbucket_help = String::from_utf8_lossy(&bitbucket_output.stdout);

    // Verify both show Bitbucket subcommands
    assert!(bb_help.contains("repo") || bb_help.contains("Repo"));
    assert!(bb_help.contains("pipeline") || bb_help.contains("Pipeline"));
    assert!(bitbucket_help.contains("repo") || bitbucket_help.contains("Repo"));
    assert!(bitbucket_help.contains("pipeline") || bitbucket_help.contains("Pipeline"));
}

#[test]
fn test_bitbucket_alias_backwards_compatible() {
    // Ensure 'bitbucket' and 'bb' behave identically (backward compatibility)
    let bitbucket_output = Command::new("cargo")
        .args(["run", "--quiet", "--", "bitbucket", "whoami"])
        .env("ATLASSIAN_CLI_PROFILE", "nonexistent")
        .output()
        .expect("Failed to execute bitbucket whoami");

    let bb_output = Command::new("cargo")
        .args(["run", "--quiet", "--", "bb", "whoami"])
        .env("ATLASSIAN_CLI_PROFILE", "nonexistent")
        .output()
        .expect("Failed to execute bb whoami");

    // Both should fail in the same way (no parsing errors, identical behavior)
    assert_eq!(
        bitbucket_output.status.success(),
        bb_output.status.success(),
        "Both commands should have identical exit status"
    );

    let bitbucket_stderr = String::from_utf8_lossy(&bitbucket_output.stderr);
    let bb_stderr = String::from_utf8_lossy(&bb_output.stderr);

    // Both should produce similar error messages (profile/auth related)
    assert_eq!(
        bitbucket_stderr, bb_stderr,
        "Both commands should produce identical error messages"
    );
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
            "run",
            "--quiet",
            "--",
            "--config",
            config_path.to_str().unwrap(),
            "bitbucket",
            "repo",
            "list",
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

/// Test that auth login help shows bearer flag and deprecation notice.
#[test]
fn test_auth_login_help_shows_bearer_flag() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "auth", "login", "--help"])
        .output()
        .expect("Failed to execute auth login help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show --bearer flag
    assert!(
        stdout.contains("--bearer"),
        "auth login help should mention --bearer flag. Got:\n{stdout}"
    );
    // Should mention deprecation
    assert!(
        stdout.contains("deprecated") || stdout.contains("App passwords"),
        "auth login help should mention app password deprecation. Got:\n{stdout}"
    );
    // Should show both basic and bearer examples
    assert!(
        stdout.contains("access token"),
        "auth login help should mention access tokens. Got:\n{stdout}"
    );
}

/// Test that bearer-only profile (no email) works for bitbucket commands.
#[test]
fn test_bitbucket_bearer_profile_no_email_required() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("config.yaml");

    // Bearer profile: no email, has bitbucket_token_type: bearer
    let config_content = r#"
default_profile: ci
profiles:
  ci:
    workspace: myworkspace
    bitbucket_token_type: bearer
"#;

    std::fs::write(&config_path, config_content).expect("Failed to write config");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "--config",
            config_path.to_str().unwrap(),
            "bitbucket",
            "repo",
            "list",
        ])
        .env("ATLASSIAN_CLI_BITBUCKET_TOKEN_CI", "fake-bearer-token")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT fail with "missing email" error
    assert!(
        !stderr.contains("missing an email"),
        "Bearer profile should not require email. Got: {stderr}"
    );

    // Expected: fails at API level (401 or connection error), not profile resolution
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
            "run",
            "--quiet",
            "--",
            "--config",
            config_path.to_str().unwrap(),
            "jira",
            "issue",
            "search",
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

#[test]
fn test_jira_issue_create_help_mentions_custom_fields() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "jira", "issue", "create", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("KEY=JSON_VALUE"),
        "expected --field value_name in help, got: {stdout}"
    );
    assert!(
        stdout.contains("jira fields list"),
        "expected fields-discovery hint in help, got: {stdout}"
    );
}

/// An invalid `ArgGroup` only panics when clap builds the command at runtime, so
/// exercising the parser is the only way to catch it. This covers the
/// `jira attachment download` group added for issue #93.
#[test]
fn test_jira_attachment_help_lists_subcommands() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "jira", "attachment", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for sub in ["list", "get", "download", "upload", "delete"] {
        assert!(
            stdout.contains(sub),
            "attachment help should list `{sub}`, got:\n{stdout}"
        );
    }
}

#[test]
fn test_jira_attachment_download_requires_a_source() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "jira", "attachment", "download"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("required"),
        "expected a required-argument error, got: {stderr}"
    );
}

#[test]
fn test_jira_attachment_download_rejects_id_with_issue() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "jira",
            "attachment",
            "download",
            "10001",
            "--issue",
            "TEST-1",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected a conflict error, got: {stderr}"
    );
}

/// `--dir` is bulk-mode only. Without an explicit conflict, clap treats the
/// `requires = "issue"` as satisfied by the ArgGroup and silently ignores it.
#[test]
fn test_jira_attachment_download_rejects_dir_without_issue() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "jira",
            "attachment",
            "download",
            "10001",
            "--dir",
            "./out",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected a conflict error, got: {stderr}"
    );
}

#[test]
fn test_jira_attachment_download_rejects_output_with_issue() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "jira",
            "attachment",
            "download",
            "--issue",
            "TEST-1",
            "--output",
            "f.bin",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected a conflict error, got: {stderr}"
    );
}
