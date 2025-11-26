use std::process::Command;

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
