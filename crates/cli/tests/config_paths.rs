//! Where the CLI puts its files, end to end (#127).
//!
//! The unit tests in `crates/config/src/paths.rs` cover resolution as a pure
//! function. These spawn the real binary against a scratch `HOME`, because the
//! thing users care about is which files appear on disk, with which permissions,
//! and that a token still decrypts after being moved.

mod common;

use std::path::Path;
use std::process::Command;

use common::Sandbox;

/// `auth login` with everything supplied, so nothing prompts.
fn login(cmd: &mut Command) -> std::process::Output {
    cmd.args([
        "auth",
        "login",
        "--profile",
        "work",
        "--base-url",
        "https://x.atlassian.net",
        "--email",
        "a@b.c",
        "--token",
        "sekrit",
    ])
    .output()
    .expect("failed to run the CLI")
}

fn assert_holds_both(dir: &Path) {
    assert!(
        dir.join("config.yaml").exists(),
        "expected config.yaml in {}",
        dir.display()
    );
    assert!(
        dir.join("credentials.enc").exists(),
        "expected credentials.enc in {}",
        dir.display()
    );
}

#[test]
fn a_fresh_install_lands_under_dot_config() {
    let sandbox = Sandbox::new();
    let out = login(&mut sandbox.bare_cli());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_holds_both(&sandbox.path(".config/atlassian-cli"));
}

#[test]
fn xdg_config_home_is_honoured() {
    let sandbox = Sandbox::new();
    let xdg = sandbox.path("xdg");
    let out = login(sandbox.bare_cli().env("XDG_CONFIG_HOME", &xdg));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_holds_both(&xdg.join("atlassian-cli"));
    assert!(!sandbox.path(".config/atlassian-cli").exists());
}

/// The basedir spec says a relative value must be ignored. Honouring it would
/// drop a credentials file into whatever directory the user is standing in.
#[test]
fn a_relative_xdg_config_home_is_ignored() {
    let sandbox = Sandbox::new();
    let out = login(sandbox.bare_cli().env("XDG_CONFIG_HOME", "relative/path"));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_holds_both(&sandbox.path(".config/atlassian-cli"));
}

#[test]
fn the_environment_variable_beats_xdg() {
    let sandbox = Sandbox::new();
    let explicit = sandbox.path("explicit");
    let out = login(
        sandbox
            .bare_cli()
            .env("XDG_CONFIG_HOME", sandbox.path("xdg"))
            .env("ATLASSIAN_CLI_CONFIG_DIR", &explicit),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_holds_both(&explicit);
    assert!(!sandbox.path("xdg/atlassian-cli").exists());
}

#[test]
fn the_flag_matches_the_environment_variable() {
    let sandbox = Sandbox::new();
    let explicit = sandbox.path("via-flag");
    let mut cmd = sandbox.bare_cli();
    cmd.arg("--config-dir").arg(&explicit);
    let out = login(&mut cmd);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_holds_both(&explicit);
}

/// The config directory holds credentials, so it should not be readable by
/// other users, and neither should the config: a profile can carry a plaintext
/// api_token.
#[cfg(unix)]
#[test]
fn the_directory_and_its_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = Sandbox::new();
    let out = login(&mut sandbox.bare_cli());
    assert!(out.status.success());

    let dir = sandbox.path(".config/atlassian-cli");
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

    assert_eq!(mode(&dir), 0o700, "config directory");
    assert_eq!(mode(&dir.join("config.yaml")), 0o600, "config.yaml");
    assert_eq!(mode(&dir.join("credentials.enc")), 0o600, "credentials.enc");
}

/// The whole point of the migration: an existing install moves without the user
/// doing anything, and the token still works afterwards.
#[test]
fn a_legacy_install_is_migrated_and_the_token_survives() {
    let sandbox = Sandbox::new();
    let legacy = sandbox.path(".atlassian-cli");

    // Seed the legacy location by pointing the override at it.
    let out = login(sandbox.bare_cli().env("ATLASSIAN_CLI_CONFIG_DIR", &legacy));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_holds_both(&legacy);

    // Now run with nothing set at all.
    let out = sandbox
        .bare_cli()
        .args(["auth", "list", "--format", "json"])
        .output()
        .expect("failed to run the CLI");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let moved = sandbox.path(".config/atlassian-cli");
    assert_holds_both(&moved);

    let archived = sandbox.path(".atlassian-cli.migrated");
    assert!(archived.exists(), "the original should have been renamed");
    assert!(
        !legacy.exists(),
        "the old path must not still resolve, or it will be edited by mistake"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Moved your configuration"), "got: {stderr}");
    assert!(stderr.contains(".migrated"), "got: {stderr}");

    // The encryption key derives from the machine id and username, not the file
    // path, so a moved credentials.enc must still decrypt. This is the assertion
    // that proves it: has_jira_token is true only if the token decrypted.
    let profiles: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(profiles[0]["name"], "work");
    assert_eq!(
        profiles[0]["has_jira_token"], true,
        "the token should still decrypt after moving: {profiles:?}"
    );
}

#[test]
fn the_migration_notice_appears_only_once() {
    let sandbox = Sandbox::new();
    login(
        sandbox
            .bare_cli()
            .env("ATLASSIAN_CLI_CONFIG_DIR", sandbox.path(".atlassian-cli")),
    );

    let first = sandbox
        .bare_cli()
        .args(["auth", "list"])
        .output()
        .expect("failed to run the CLI");
    assert!(String::from_utf8_lossy(&first.stderr).contains("Moved your configuration"));

    let second = sandbox
        .bare_cli()
        .args(["auth", "list"])
        .output()
        .expect("failed to run the CLI");
    assert!(
        second.stderr.is_empty(),
        "a second run should say nothing, got: {}",
        String::from_utf8_lossy(&second.stderr)
    );
}

/// A leftover empty directory is not an install. Migrating it would rename a
/// directory the user may have made on purpose, for nothing.
#[test]
fn an_empty_legacy_directory_is_left_alone() {
    let sandbox = Sandbox::new();
    let legacy = sandbox.path(".atlassian-cli");
    std::fs::create_dir_all(&legacy).unwrap();

    let out = login(&mut sandbox.bare_cli());
    assert!(out.status.success());

    assert_holds_both(&sandbox.path(".config/atlassian-cli"));
    assert!(legacy.exists(), "the empty directory should be untouched");
    assert!(!sandbox.path(".atlassian-cli.migrated").exists());
}

/// An explicit choice is not second-guessed: pointing the variable at the legacy
/// directory keeps using it, with no migration.
#[test]
fn an_explicit_directory_is_never_migrated() {
    let sandbox = Sandbox::new();
    let legacy = sandbox.path(".atlassian-cli");

    login(sandbox.bare_cli().env("ATLASSIAN_CLI_CONFIG_DIR", &legacy));
    let out = sandbox
        .bare_cli()
        .env("ATLASSIAN_CLI_CONFIG_DIR", &legacy)
        .args(["auth", "list"])
        .output()
        .expect("failed to run the CLI");

    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_holds_both(&legacy);
    assert!(!sandbox.path(".atlassian-cli.migrated").exists());
}

/// `--config` moves one file. Credentials follow the directory, so a shared or
/// read-only config file does not imply writing secrets next to it.
#[test]
fn the_config_flag_moves_only_the_config_file() {
    let sandbox = Sandbox::new();
    let dir = sandbox.path("dir");
    login(sandbox.bare_cli().env("ATLASSIAN_CLI_CONFIG_DIR", &dir));

    let elsewhere = sandbox.path("elsewhere.yaml");
    std::fs::write(
        &elsewhere,
        "default_profile: other\nprofiles:\n  other:\n    email: z@z.z\n    base_url: https://other.atlassian.net\n",
    )
    .unwrap();

    let out = sandbox
        .bare_cli()
        .env("ATLASSIAN_CLI_CONFIG_DIR", &dir)
        .arg("--config")
        .arg(&elsewhere)
        .args(["auth", "list", "--all", "--format", "json"])
        .output()
        .expect("failed to run the CLI");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let profiles: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        profiles[0]["name"], "other",
        "the profile should come from the --config file"
    );
    assert!(
        dir.join("credentials.enc").exists(),
        "credentials should stay in the config directory"
    );
    assert!(
        !sandbox.path("credentials.enc").exists(),
        "credentials must not follow the config file's parent"
    );
}

/// The older directory is still found, and lands directly in the new location
/// rather than hopping through ~/.atlassian-cli on the way.
#[test]
fn the_oldest_legacy_directory_migrates_in_one_step() {
    let sandbox = Sandbox::new();
    let atlcli = sandbox.path(".atlcli");
    std::fs::create_dir_all(&atlcli).unwrap();
    std::fs::write(
        atlcli.join("config.yaml"),
        "default_profile: old\nprofiles:\n  old:\n    email: o@o.o\n    base_url: https://old.atlassian.net\n",
    )
    .unwrap();

    let out = sandbox
        .bare_cli()
        .args(["auth", "list", "--all", "--format", "json"])
        .output()
        .expect("failed to run the CLI");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(sandbox.path(".config/atlassian-cli/config.yaml").exists());
    assert!(
        !sandbox.path(".atlassian-cli").exists(),
        "there should be no intermediate hop"
    );
    assert!(sandbox.path(".atlcli.migrated").exists());

    let profiles: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(profiles[0]["name"], "old");
}
