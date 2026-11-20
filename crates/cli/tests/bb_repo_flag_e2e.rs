//! `--repo` and the Git remote on Bitbucket commands, through the built binary.
//!
//! Bitbucket commands always call `https://api.bitbucket.org`, so a request
//! cannot go to a mock server from here. Each case below stops before a
//! request: `pr diff` prints an address and sends nothing, `branch delete`
//! refuses without a terminal, and the others fail in the argument handling.
//! The requests are covered in-process, in
//! `crates/cli/src/commands/bitbucket/repo_argument_tests.rs`.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::Sandbox;

struct Fixture {
    sandbox: Sandbox,
    config: PathBuf,
    /// A directory whose Git remote names `myworkspace/web_app`.
    checkout: PathBuf,
    /// A directory that is not in a Git repository.
    plain: PathBuf,
}

fn git(dir: &Path, home: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git must be installed to run this test");
    assert!(
        status.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

fn fixture() -> Fixture {
    let sandbox = Sandbox::new();
    let config = sandbox.path("config.yaml");
    std::fs::write(
        &config,
        "default_profile: local\n\
         profiles:\n\
         \x20 local:\n\
         \x20   email: dev@example.com\n\
         \x20   base_url: https://example.atlassian.net\n\
         \x20   workspace: myworkspace\n",
    )
    .unwrap();

    let checkout = sandbox.path("checkout");
    let plain = sandbox.path("plain");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::create_dir_all(&plain).unwrap();
    git(&checkout, sandbox.home(), &["init", "-q"]);
    git(
        &checkout,
        sandbox.home(),
        &[
            "remote",
            "add",
            "origin",
            "git@bitbucket.org:myworkspace/web_app.git",
        ],
    );

    Fixture {
        sandbox,
        config,
        checkout,
        plain,
    }
}

impl Fixture {
    fn run(&self, dir: &Path, args: &[&str]) -> Output {
        self.sandbox
            .cli()
            .arg("--config")
            .arg(&self.config)
            .env("ATLASSIAN_CLI_TOKEN_LOCAL", "fake-token")
            .env("ATLASSIAN_CLI_BITBUCKET_TOKEN_LOCAL", "fake-token")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("failed to run the CLI")
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

const DIFF_ADDRESS: &str = "https://bitbucket.org/myworkspace/web_app/pull-requests/42/diff";

/// The failed command from the report, with a subcommand that sends nothing.
#[test]
fn pr_diff_accepts_repo_as_a_flag_before_the_id() {
    let fx = fixture();
    let out = fx.run(&fx.plain, &["bb", "pr", "diff", "--repo", "web_app", "42"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains(DIFF_ADDRESS), "{}", stdout(&out));
}

#[test]
fn pr_diff_keeps_the_positional_form() {
    let fx = fixture();
    let out = fx.run(&fx.plain, &["bb", "pr", "diff", "web_app", "42"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains(DIFF_ADDRESS), "{}", stdout(&out));
}

#[test]
fn pr_diff_reads_the_repo_from_the_git_remote() {
    let fx = fixture();
    let out = fx.run(&fx.checkout, &["bb", "pr", "diff", "42"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains(DIFF_ADDRESS), "{}", stdout(&out));
}

/// `pr get` itself must reach the repository check, not fail in clap.
#[test]
fn pr_get_without_a_repo_asks_for_one() {
    let fx = fixture();
    let out = fx.run(&fx.plain, &["bb", "pr", "get", "42"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("Repository required for 'pr get'"), "{err}");
    assert!(!err.contains("unexpected argument"), "{err}");
}

#[test]
fn a_positional_repo_that_disagrees_with_the_flag_is_an_error() {
    let fx = fixture();
    let out = fx.run(
        &fx.plain,
        &["bb", "pr", "get", "web_app", "42", "--repo", "other_repo"],
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains(
            "Repository given twice: 'web_app' as an argument and 'other_repo' with --repo."
        ),
        "{}",
        stderr(&out)
    );
}

/// `branch delete` with `--repo` reaches the confirmation, which names the
/// repository and refuses without a terminal.
#[test]
fn branch_delete_accepts_repo_as_a_flag_and_keeps_its_confirmation() {
    let fx = fixture();
    let out = fx.run(
        &fx.plain,
        &["bb", "branch", "delete", "--repo", "web_app", "feature-x"],
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("About to delete branch feature-x from myworkspace/web_app."),
        "{err}"
    );
    assert!(err.contains("no terminal available to confirm"), "{err}");
}

/// A command that deletes must not take its target from the directory.
#[test]
fn branch_delete_does_not_read_the_repo_from_the_git_remote() {
    let fx = fixture();
    let out = fx.run(
        &fx.checkout,
        &["bb", "branch", "delete", "feature-x", "--force"],
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("Repository required for 'branch delete'"),
        "{err}"
    );
    assert!(
        err.contains("does not read the repository from the Git remote"),
        "{err}"
    );
    assert!(err.contains("The Git remote names 'web_app'"), "{err}");
}

#[test]
fn the_help_shows_the_repo_as_optional() {
    let fx = fixture();
    let out = fx.run(&fx.plain, &["bb", "pr", "get", "--help"]);
    assert!(out.status.success());
    let help = stdout(&out);
    assert!(
        help.contains("Usage: atlassian-cli bitbucket pr get [OPTIONS] [REPO] <PR_ID>"),
        "{help}"
    );
    assert!(help.contains("--repo <REPO>"), "{help}");
}
