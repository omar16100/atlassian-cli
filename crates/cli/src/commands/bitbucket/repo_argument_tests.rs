//! Each Bitbucket command that takes a repository accepts it as an argument,
//! with `--repo`, or from the Git remote.
//!
//! Each test starts from a command line, parses it with clap, runs the
//! dispatch against a mock server, and checks the path that the server
//! received. A parse test alone cannot show that the repository reaches the
//! request.

use anyhow::Result;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::{OutputFormat, OutputRenderer};
use clap::Parser;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::git::GitContext;
use super::BitbucketArgs;

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: BitbucketArgs,
}

/// Where a command can read the repository from when no value is given.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    /// Argument, `--repo`, or the Git remote.
    AnyRemote,
    /// Argument or `--repo` only. The command deletes or revokes.
    ExplicitOnly,
}
use Source::{AnyRemote, ExplicitOnly};

/// Every command that takes a repository: the words before the repository,
/// the words after it, and where the repository can come from.
const COMMANDS: &[(&[&str], &[&str], Source)] = &[
    (&["repo", "get"], &[], AnyRemote),
    (&["repo", "update"], &["--name", "Web app"], AnyRemote),
    (&["repo", "create"], &[], ExplicitOnly),
    (&["repo", "delete"], &["--force"], ExplicitOnly),
    (&["branch", "list"], &[], AnyRemote),
    (&["branch", "get"], &["main"], AnyRemote),
    (
        &["branch", "create"],
        &["feature-x", "--from", "main"],
        AnyRemote,
    ),
    (
        &["branch", "delete"],
        &["feature-x", "--force"],
        ExplicitOnly,
    ),
    (
        &["branch", "protect"],
        &["--pattern", "main", "--kind", "push"],
        AnyRemote,
    ),
    (&["branch", "unprotect"], &["7"], ExplicitOnly),
    (&["branch", "restrictions"], &[], AnyRemote),
    (&["pr", "list"], &[], AnyRemote),
    (&["pr", "get"], &["42"], AnyRemote),
    (
        &["pr", "create"],
        &[
            "--title",
            "T",
            "--source",
            "feature-x",
            "--destination",
            "main",
        ],
        AnyRemote,
    ),
    (&["pr", "update"], &["42", "--title", "T"], AnyRemote),
    (&["pr", "merge"], &["42"], AnyRemote),
    (&["pr", "decline"], &["42"], AnyRemote),
    (&["pr", "approve"], &["42"], AnyRemote),
    (&["pr", "unapprove"], &["42"], AnyRemote),
    (&["pr", "diff"], &["42"], AnyRemote),
    (&["pr", "comments"], &["42"], AnyRemote),
    (&["pr", "comment"], &["42", "--text", "LGTM"], AnyRemote),
    (&["pr", "resolve-comment"], &["42", "9"], AnyRemote),
    (&["pr", "reopen-comment"], &["42", "9"], AnyRemote),
    (&["pr", "reviewers"], &["42"], AnyRemote),
    (&["webhook", "list"], &[], AnyRemote),
    (
        &["webhook", "create"],
        &["--url", "https://example.com/hook"],
        AnyRemote,
    ),
    (&["webhook", "delete"], &["{hook-1}"], ExplicitOnly),
    (&["ssh-key", "list"], &[], AnyRemote),
    (
        &["ssh-key", "add"],
        &["--label", "ci", "--key", "ssh-ed25519 AAAA"],
        AnyRemote,
    ),
    (&["ssh-key", "delete"], &["{key-1}"], ExplicitOnly),
    (&["permission", "list"], &[], AnyRemote),
    (
        &["permission", "grant"],
        &["--user-uuid", "{acc-1}", "--permission", "read"],
        AnyRemote,
    ),
    (&["permission", "revoke"], &["{acc-1}"], ExplicitOnly),
    (&["commit", "list"], &[], AnyRemote),
    (&["commit", "get"], &["abc123"], AnyRemote),
    (&["commit", "diff"], &["abc123"], AnyRemote),
    (&["commit", "browse"], &["--commit", "main"], AnyRemote),
    (&["bulk", "delete-branches"], &[], ExplicitOnly),
];

const REPO_PATH: &str = "/2.0/repositories/myworkspace/web_app";

struct Outcome {
    result: Result<()>,
    /// Method and path of each request that the server received.
    requests: Vec<(String, String)>,
}

impl Outcome {
    fn error(&self) -> String {
        match &self.result {
            Ok(()) => String::new(),
            Err(err) => format!("{err:#}"),
        }
    }
}

/// Parse `argv`, run the dispatch against a mock server, and collect the
/// requests. `git_repo` is the repository that the Git remote names.
async fn run(argv: &[&str], git_repo: Option<&str>) -> Outcome {
    let server = MockServer::start().await;
    // One body that the pull request model and the list models can read.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 42,
            "title": "Add a page",
            "state": "OPEN",
            "author": {"display_name": "Dev"},
            "source": {"branch": {"name": "feature-x"}},
            "destination": {"branch": {"name": "main"}},
            "values": []
        })))
        .mount(&server)
        .await;
    for verb in ["POST", "PUT", "DELETE"] {
        Mock::given(method(verb))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
    }

    let result = match Cli::try_parse_from(std::iter::once("bb").chain(argv.iter().copied())) {
        Err(err) => Err(anyhow::anyhow!("clap: {err}")),
        Ok(cli) => {
            let renderer = OutputRenderer::new(OutputFormat::Json);
            let git_ctx = GitContext {
                workspace: None,
                repo_slug: git_repo.map(String::from),
            };
            super::execute_in(
                cli.args,
                ApiClient::new(server.uri()).unwrap(),
                &renderer,
                Some("myworkspace"),
                false,
                git_ctx,
            )
            .await
        }
    };

    let requests = server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.method.to_string(), r.url.path().to_string()))
        .collect();
    Outcome { result, requests }
}

fn join(parts: &[&[&str]]) -> Vec<String> {
    parts
        .iter()
        .flat_map(|p| p.iter().map(|s| s.to_string()))
        .collect()
}

async fn run_owned(argv: &[String], git_repo: Option<&str>) -> Outcome {
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    run(&argv, git_repo).await
}

/// The first request must go to the repository `web_app`.
///
/// `pr diff` sends no request: it prints the address of the diff. For that
/// command the check is that the command accepts the repository.
fn assert_targets_web_app(outcome: &Outcome, argv: &[String]) {
    let words: Vec<&str> = argv.iter().map(String::as_str).collect();
    if words.windows(2).any(|pair| pair == ["pr", "diff"]) {
        assert!(outcome.result.is_ok(), "{argv:?}: {}", outcome.error());
        assert!(outcome.requests.is_empty(), "{argv:?}");
        return;
    }
    let first = outcome.requests.first().unwrap_or_else(|| {
        panic!(
            "{argv:?} sent no request. Result: {}",
            if outcome.result.is_ok() {
                "ok".to_string()
            } else {
                outcome.error()
            }
        )
    });
    assert!(
        first.1 == REPO_PATH || first.1.starts_with(&format!("{REPO_PATH}/")),
        "{argv:?} sent {} {}",
        first.0,
        first.1
    );
}

/// The exact command from the failure report.
#[tokio::test]
async fn pr_get_accepts_repo_as_a_flag_before_the_id() {
    let outcome = run(&["pr", "get", "--repo", "web_app", "42"], None).await;
    outcome.result.as_ref().expect("the command must succeed");
    assert_eq!(
        outcome.requests,
        vec![("GET".to_string(), format!("{REPO_PATH}/pullrequests/42"))]
    );
}

#[tokio::test]
async fn pr_get_reads_the_repo_from_the_git_remote() {
    let outcome = run(&["pr", "get", "42"], Some("web_app")).await;
    outcome.result.as_ref().expect("the command must succeed");
    assert_eq!(
        outcome.requests,
        vec![("GET".to_string(), format!("{REPO_PATH}/pullrequests/42"))]
    );
}

/// The form that worked before must give the same request.
#[tokio::test]
async fn pr_get_keeps_the_positional_form() {
    let outcome = run(&["pr", "get", "web_app", "42"], Some("other_repo")).await;
    outcome.result.as_ref().expect("the command must succeed");
    assert_eq!(
        outcome.requests,
        vec![("GET".to_string(), format!("{REPO_PATH}/pullrequests/42"))]
    );
}

#[tokio::test]
async fn every_command_keeps_the_positional_repo() {
    for (head, tail, _) in COMMANDS {
        let argv = join(&[head, &["web_app"], tail]);
        let outcome = run_owned(&argv, None).await;
        assert_targets_web_app(&outcome, &argv);
    }
}

#[tokio::test]
async fn every_command_accepts_repo_as_a_flag_after_the_subcommand() {
    for (head, tail, _) in COMMANDS {
        let argv = join(&[head, &["--repo", "web_app"], tail]);
        let outcome = run_owned(&argv, None).await;
        assert_targets_web_app(&outcome, &argv);
    }
}

#[tokio::test]
async fn every_command_accepts_repo_as_a_flag_before_the_subcommand() {
    for (head, tail, _) in COMMANDS {
        let argv = join(&[&["--repo", "web_app"], head, tail]);
        let outcome = run_owned(&argv, None).await;
        assert_targets_web_app(&outcome, &argv);
    }
}

#[tokio::test]
async fn every_command_accepts_repo_as_a_flag_at_the_end() {
    for (head, tail, _) in COMMANDS {
        let argv = join(&[head, tail, &["--repo", "web_app"]]);
        let outcome = run_owned(&argv, None).await;
        assert_targets_web_app(&outcome, &argv);
    }
}

/// An explicit `--repo` wins over the Git remote.
#[tokio::test]
async fn the_repo_flag_wins_over_the_git_remote() {
    for (head, tail, _) in COMMANDS {
        let argv = join(&[head, tail, &["--repo", "web_app"]]);
        let outcome = run_owned(&argv, Some("other_repo")).await;
        assert_targets_web_app(&outcome, &argv);
    }
}

#[tokio::test]
async fn commands_that_do_not_delete_read_the_repo_from_the_git_remote() {
    for (head, tail, source) in COMMANDS {
        if *source != AnyRemote {
            continue;
        }
        let argv = join(&[head, tail]);
        let outcome = run_owned(&argv, Some("web_app")).await;
        assert_targets_web_app(&outcome, &argv);
    }
}

/// A command that deletes or revokes must not take its target from the
/// directory that the user is in.
#[tokio::test]
async fn commands_that_delete_do_not_read_the_repo_from_the_git_remote() {
    for (head, tail, source) in COMMANDS {
        if *source != ExplicitOnly {
            continue;
        }
        let argv = join(&[head, tail]);
        let outcome = run_owned(&argv, Some("web_app")).await;
        assert!(outcome.result.is_err(), "{argv:?} must fail");
        let error = outcome.error();
        assert!(
            error.contains("Repository required") && error.contains("Git remote"),
            "{argv:?}: {error}"
        );
        assert!(
            outcome.requests.is_empty(),
            "{argv:?} sent {:?}",
            outcome.requests
        );
    }
}

#[tokio::test]
async fn a_missing_repo_is_an_error_that_names_the_command() {
    for (head, tail, _) in COMMANDS {
        let argv = join(&[head, tail]);
        let outcome = run_owned(&argv, None).await;
        let error = outcome.error();
        let name = head.join(" ");
        assert!(
            error.contains(&format!("Repository required for '{name}'")),
            "{argv:?}: {error}"
        );
        assert!(outcome.requests.is_empty(), "{argv:?}");
    }
}

/// The same rule and the same text as the pipeline commands.
#[tokio::test]
async fn a_positional_repo_that_disagrees_with_the_flag_is_an_error() {
    for (head, tail, _) in COMMANDS {
        let argv = join(&[head, &["web_app"], tail, &["--repo", "other_repo"]]);
        let outcome = run_owned(&argv, Some("web_app")).await;
        assert_eq!(
            outcome.error(),
            "Repository given twice: 'web_app' as an argument and 'other_repo' with --repo.",
            "{argv:?}"
        );
        assert!(outcome.requests.is_empty(), "{argv:?}");
    }
}

#[tokio::test]
async fn the_identifiers_after_the_repo_reach_the_request() {
    let cases: &[(&[&str], &str, &str)] = &[
        (
            &["pr", "resolve-comment", "--repo", "web_app", "42", "9"],
            "POST",
            "/pullrequests/42/comments/9/resolve",
        ),
        (
            &["pr", "reopen-comment", "42", "9", "--repo", "web_app"],
            "DELETE",
            "/pullrequests/42/comments/9/resolve",
        ),
        (
            &["pr", "approve", "--repo", "web_app", "42"],
            "POST",
            "/pullrequests/42/approve",
        ),
        (
            &["branch", "get", "--repo", "web_app", "main"],
            "GET",
            "/refs/branches/main",
        ),
        (
            &["commit", "get", "--repo", "web_app", "abc123"],
            "GET",
            "/commit/abc123",
        ),
        (
            &["branch", "unprotect", "--repo", "web_app", "7"],
            "DELETE",
            "/branch-restrictions/7",
        ),
        (
            &["webhook", "delete", "--repo", "web_app", "hook-1"],
            "DELETE",
            "/hooks/hook-1",
        ),
    ];
    for (argv, verb, suffix) in cases {
        let outcome = run(argv, None).await;
        assert_eq!(
            outcome.requests.first(),
            Some(&(verb.to_string(), format!("{REPO_PATH}{suffix}"))),
            "{argv:?}: {}",
            outcome.error()
        );
    }
}

/// One value where a number must be is a slug in the wrong place. The error
/// must say so, and not ask for a repository.
#[tokio::test]
async fn a_slug_where_the_pull_request_id_must_be_is_named() {
    let outcome = run(&["pr", "get", "web_app"], None).await;
    let error = outcome.error();
    assert!(
        error.contains("'web_app'") && error.contains("PR_ID") && error.contains("[REPO] <PR_ID>"),
        "{error}"
    );
    assert!(outcome.requests.is_empty());
}

#[tokio::test]
async fn too_few_or_too_many_values_are_rejected() {
    for argv in [
        &["pr", "get"][..],
        &["pr", "get", "web_app", "42", "43"],
        &["pr", "resolve-comment", "--repo", "web_app", "42"],
        &["pr", "resolve-comment", "web_app", "42", "9", "10"],
        &["branch", "get"],
        &["branch", "list", "web_app", "extra"],
    ] {
        let outcome = run(argv, Some("web_app")).await;
        assert!(outcome.result.is_err(), "{argv:?} must fail");
        assert!(outcome.requests.is_empty(), "{argv:?}");
    }
}

/// A numeric slug is a repository when the count says so.
#[tokio::test]
async fn a_numeric_repo_slug_is_recognised_by_count() {
    let outcome = run(&["pr", "get", "123", "42"], None).await;
    assert_eq!(
        outcome.requests,
        vec![(
            "GET".to_string(),
            "/2.0/repositories/myworkspace/123/pullrequests/42".to_string()
        )]
    );
}

/// clap cannot show that only the first value of a list is optional, so each
/// command with a list has a usage line that is written out. This test keeps
/// each of those lines equal to the command path, the required options and the
/// value names.
#[test]
fn usage_lines_match_the_command_tree() {
    use clap::CommandFactory;

    let root = Cli::command();
    let mut checked = 0;
    for group in root.get_subcommands() {
        if group.get_name() == "pipeline" {
            continue;
        }
        for leaf in group.get_subcommands() {
            let Some(list) = leaf.get_arguments().find(|a| a.get_id() == "targets") else {
                continue;
            };
            let mut expected = format!(
                "Usage: atlassian-cli bitbucket {} {} [OPTIONS] ",
                group.get_name(),
                leaf.get_name()
            );
            for option in leaf
                .get_arguments()
                .filter(|a| a.is_required_set() && !a.is_positional())
            {
                let value = option.get_value_names().expect("a value name")[0].to_string();
                expected.push_str(&format!("--{} <{value}> ", option.get_long().unwrap()));
            }
            let names = list.get_value_names().expect("value names");
            assert_eq!(names[0].to_string(), "REPO");
            expected.push_str("[REPO]");
            for name in &names[1..] {
                expected.push_str(&format!(" <{name}>"));
            }
            assert_eq!(leaf.clone().render_usage().to_string(), expected);
            checked += 1;
        }
    }
    assert_eq!(checked, 21, "commands with a [REPO] list");
}

/// A subcommand argument with the id `repo` or `workspace` hides the global
/// option of that name on the command. That was the cause of
/// "unexpected argument '--repo'".
#[test]
fn no_command_hides_the_global_repo_or_workspace() {
    use clap::CommandFactory;

    fn walk(cmd: &clap::Command, path: &str, found: &mut Vec<String>) {
        for sub in cmd.get_subcommands() {
            let path = format!("{path} {}", sub.get_name());
            for arg in sub.get_arguments() {
                let id = arg.get_id().as_str();
                if id == "repo" || id == "workspace" {
                    found.push(format!("{path}: `{id}`"));
                }
            }
            walk(sub, &path, found);
        }
    }

    // `Cli::command()` is not built, so the globals are not yet copied into
    // the subcommands. Each argument found here is a local declaration.
    let mut found = Vec::new();
    walk(&Cli::command(), "bb", &mut found);
    assert!(found.is_empty(), "{}", found.join("\n"));
}
