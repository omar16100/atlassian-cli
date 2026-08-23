//! Isolation for tests that spawn the CLI.
//!
//! Several tests run the binary with no `--config`, which means they read and
//! write the developer's real `~/.atlassian-cli`. Today the worst case is a
//! stray file copy from the `.atlcli` migration. Once the config directory can
//! move, the worst case becomes `cargo test` relocating a real configuration,
//! and `auth login` runs a credential migration that deletes a plaintext
//! credentials file. So every spawn goes through here.
//!
//! `HOME` is redirected as well as `ATLASSIAN_CLI_CONFIG_DIR`, because the two
//! cover different eras: `HOME` is what the current code resolves against, and
//! the config-dir variable is what it will resolve against. Setting both means
//! this file protects the suite before and after that change.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// The compiled binary, rather than `cargo run`. Going through cargo would mean
/// either leaving `HOME` alone, which defeats the isolation, or redirecting it
/// and sending cargo looking for `~/.cargo` inside a temp directory.
pub const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

/// A scratch home directory for one test.
pub struct Sandbox {
    home: TempDir,
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            home: TempDir::new().expect("failed to create a scratch home"),
        }
    }

    pub fn home(&self) -> &Path {
        self.home.path()
    }

    /// Where the CLI is told to keep its files. Not created: a test that wants
    /// it populated should run a command that writes there.
    pub fn config_dir(&self) -> PathBuf {
        self.home.path().join("config-dir")
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.home.path().join(rel)
    }

    /// A command pointed at this sandbox, with any real credentials in the
    /// environment removed so a test can neither read nor be influenced by them.
    pub fn cli(&self) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.env("HOME", self.home.path())
            .env("ATLASSIAN_CLI_CONFIG_DIR", self.config_dir())
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("ATLASSIAN_API_TOKEN")
            .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
            .env_remove("BITBUCKET_TOKEN");
        cmd
    }

    /// A command with `HOME` pointed here but nothing else set, for tests that
    /// exercise path resolution itself and need to choose their own variables.
    pub fn bare_cli(&self) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.env("HOME", self.home.path())
            .env_remove("ATLASSIAN_CLI_CONFIG_DIR")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("ATLASSIAN_API_TOKEN")
            .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
            .env_remove("BITBUCKET_TOKEN");
        cmd
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}
