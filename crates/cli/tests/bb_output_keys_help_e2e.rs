//! `--help` of the pull request and pipeline commands lists the output keys.
//!
//! Scripts read `source`, `destination` and `author` as objects, because the
//! Bitbucket REST API returns objects for those names, and they failed. No help
//! text said which keys exist. These run the built binary, because the help
//! that a caller reads is the product here.

mod common;

use common::Sandbox;

fn long_help(args: &[&str]) -> String {
    let output = Sandbox::new()
        .cli()
        .args(args)
        .arg("--help")
        .output()
        .expect("failed to run the CLI");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The lines of the key table, without the alignment.
fn key_names(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|line| !line.starts_with("Output keys"))
        .skip(1)
        .take_while(|line| line.starts_with("  ") && !line.starts_with("  * "))
        .map(|line| {
            line.split_whitespace()
                .next()
                .unwrap()
                .trim_end_matches('*')
                .to_string()
        })
        .collect()
}

#[test]
fn pr_list_help_lists_the_output_keys() {
    let help = long_help(&["bitbucket", "pr", "list"]);
    assert_eq!(
        key_names(&help),
        ["id", "title", "state", "author", "source", "destination"],
        "{help}"
    );
    assert!(
        help.contains("Source branch name (not an object)"),
        "{help}"
    );
    assert!(
        help.contains("bitbucket api /2.0/repositories/<workspace>/<repo>/pullrequests"),
        "{help}"
    );
}

#[test]
fn pr_get_help_lists_the_output_keys_and_the_reviewer_keys() {
    let help = long_help(&["bitbucket", "pr", "get"]);
    let keys = key_names(&help);
    for key in ["id", "source", "destination", "approvals", "reviewers"] {
        assert!(keys.iter().any(|k| k == key), "{key} is not in {keys:?}");
    }
    assert!(help.contains("Keys of each `reviewers` entry:"), "{help}");
    assert!(help.contains("  participated_on  "), "{help}");
}

#[test]
fn pipeline_help_lists_the_output_keys() {
    let list = long_help(&["bitbucket", "pipeline", "list"]);
    assert_eq!(
        key_names(&list),
        [
            "build_number",
            "state",
            "ref_name",
            "commit",
            "target_type",
            "created",
            "steps_summary",
            "pending_manual_steps",
            "commit_hash"
        ],
        "{list}"
    );

    let get = long_help(&["bitbucket", "pipeline", "get"]);
    let keys = key_names(&get);
    assert_eq!(keys.first().map(String::as_str), Some("uuid"), "{get}");
    assert_eq!(
        keys.last().map(String::as_str),
        Some("commit_hash"),
        "{get}"
    );

    let steps = long_help(&["bitbucket", "pipeline", "steps"]);
    assert_eq!(
        key_names(&steps),
        [
            "uuid",
            "name",
            "status",
            "started",
            "completed",
            "duration",
            "logs_url",
            "trigger"
        ],
        "{steps}"
    );
}

/// clap must not wrap a line of the key table: a wrapped line starts in the
/// first column and breaks the table.
#[test]
fn the_key_tables_are_not_wrapped() {
    for args in [
        &["bitbucket", "pr", "list"][..],
        &["bitbucket", "pr", "get"],
        &["bitbucket", "pipeline", "list"],
        &["bitbucket", "pipeline", "get"],
        &["bitbucket", "pipeline", "steps"],
    ] {
        let help = long_help(args);
        let table: Vec<&str> = help
            .lines()
            .skip_while(|line| !line.starts_with("Output keys"))
            .skip(1)
            .take_while(|line| !line.is_empty())
            .collect();
        assert!(table.len() >= 6, "{args:?}: {help}");
        for line in table {
            assert!(line.starts_with("  "), "{args:?}: wrapped line: {line:?}");
        }
    }
}

/// The short help stays one line.
#[test]
fn short_help_stays_short() {
    let output = Sandbox::new()
        .cli()
        .args(["bitbucket", "pr", "list", "-h"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("List pull requests for a repository\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("Output keys"), "{stdout}");
}
