//! Checks that every command line printed in `README.md` actually parses.
//!
//! The README drifted badly before this existed: 45 of its 128 examples had
//! stale syntax (`jira get` after the `issue` group was introduced, `--id` for
//! arguments that became positional, `--output json` where the flag is
//! `--format`). Documentation that does not run is worse than none, and a
//! reader has no way to tell which half they are looking at.
//!
//! Only the argument parse is exercised. Commands are pointed at an unroutable
//! localhost port, so nothing reaches the network: clap rejects a malformed
//! command line with exit code 2 before any request is attempted.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");

/// Command lines in the README, one per line, ignoring continuations and
/// anything containing a shell pipeline (which is not a single argv).
fn readme_commands() -> Vec<String> {
    let readme = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../README.md")
        .canonicalize()
        .expect("README.md not found");
    let text = std::fs::read_to_string(readme).expect("failed to read README.md");

    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("atlassian-cli ") || line.ends_with('\\') {
            continue;
        }
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.contains('|') || out.contains(&line.to_string()) {
            continue;
        }
        out.push(line.to_string());
    }
    out
}

/// Minimal shell-style split: enough for the quoting the README uses.
fn split_args(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;

    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started || !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => current.push(c),
        }
    }
    if started || !current.is_empty() {
        args.push(current);
    }
    args
}

#[test]
fn every_readme_command_parses() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.yaml");
    // Port 1 refuses instantly, so a command that parses fails at connect
    // rather than doing anything.
    std::fs::write(
        &config,
        "default_profile: t\nprofiles:\n  t:\n    email: a@b.c\n    base_url: http://127.0.0.1:1\n    workspace: w\n",
    )
    .unwrap();

    let commands = readme_commands();
    assert!(
        commands.len() > 100,
        "expected to find the README's command examples, found {}",
        commands.len()
    );

    // One process per example, so spawn them all first and collect after.
    // Sequentially this takes about 100s, which is not worth it in CI.
    let children: Vec<_> = commands
        .iter()
        .map(|line| {
            let args = split_args(line);
            let child = Command::new(BIN)
                .arg("--config")
                .arg(&config)
                .args(&args[1..])
                .env("ATLASSIAN_CLI_TOKEN_T", "x")
                .env("ATLASSIAN_CLI_BITBUCKET_TOKEN_T", "x")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("failed to run the CLI");
            (line, child)
        })
        .collect();

    let mut failures = Vec::new();
    for (line, child) in children {
        let output = child
            .wait_with_output()
            .expect("failed to wait for the CLI");
        // clap exits 2 on a usage error. Anything else means the command line
        // was accepted, which is all this test cares about.
        if output.status.code() == Some(2) {
            let reason = String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            failures.push(format!("  {line}\n      -> {reason}"));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} README commands do not parse:\n{}",
        failures.len(),
        commands.len(),
        failures.join("\n")
    );
}
