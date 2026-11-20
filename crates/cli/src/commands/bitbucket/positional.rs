//! A leading repository among a pipeline command's positional arguments.
//!
//! `bb pr get <repo> 54` took the repository positionally while
//! `bb pipeline list <repo>` rejected it and wanted `--repo`. Pipeline commands
//! now accept `[REPO]` in front of their identifiers, alongside `--repo` and
//! the git remote.
//!
//! The pipeline and step identifiers were already optional positionals, so a
//! single value has to be told apart from a repository. The count decides
//! first: a value beyond the identifiers the command can still take is the
//! repository. Otherwise the first value is the repository only when it cannot
//! be a pipeline identifier (all digits, or a UUID). A repository whose slug is
//! all digits therefore needs `--repo`, or all of its identifiers written out.
//!
//! The other command groups (`pr`, `branch`, `commit` and more) use the same
//! `[REPO]` argument and the same `choose_repo`. Their arguments after the
//! repository are all required, so `split_repo_and_required` decides by count
//! only.

use anyhow::{bail, Result};

/// The positional values of one command, split.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Positionals {
    pub(super) repo: Option<String>,
    /// Identifiers for the slots that no flag filled, in order.
    pub(super) ids: Vec<String>,
}

/// A build number or a pipeline/step UUID, braced or bare.
pub(super) fn is_pipeline_identifier(value: &str) -> bool {
    if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let bare = value
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .unwrap_or(value);
    let groups: Vec<&str> = bare.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, len)| g.len() == len && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Split `values` into an optional repository and identifiers.
///
/// `slots` names the identifiers the command takes positionally, in order
/// (`["PIPELINE", "STEP_UUID"]` for `logs`); `filled` says which of them a
/// flag such as `--pipeline` already supplied.
pub(super) fn split_leading_repo(
    values: Vec<String>,
    slots: &[&str],
    filled: &[bool],
) -> Result<Positionals> {
    let available = slots
        .iter()
        .zip(filled.iter().chain(std::iter::repeat(&false)))
        .filter(|(_, f)| !**f)
        .count();
    let usage = std::iter::once("[REPO]".to_string())
        .chain(slots.iter().map(|s| format!("[{s}]")))
        .collect::<Vec<_>>()
        .join(" ");

    if values.len() > available + 1 {
        bail!(
            "Too many arguments: expected {usage}, got {} values ({}).",
            values.len(),
            values.join(" ")
        );
    }

    let first_is_repo = match values.first() {
        None => false,
        Some(first) if values.len() == available + 1 => {
            // One value more than the free slots: the count says the first is
            // the repository. Unless a flag took a slot and the value looks
            // exactly like what that flag holds, which is far more likely a
            // doubled identifier than a numeric repository slug.
            if available < slots.len() && is_pipeline_identifier(first) {
                bail!(
                    "'{first}' looks like a pipeline identifier, but the identifier was also given \
                     as a flag. Pass the repository with --repo if its slug is '{first}'."
                );
            }
            true
        }
        Some(first) => !is_pipeline_identifier(first),
    };

    let mut values = values.into_iter();
    let repo = if first_is_repo { values.next() } else { None };
    Ok(Positionals {
        repo,
        ids: values.collect(),
    })
}

/// The positional values of a command whose arguments after the repository
/// are all required (`bb pr get [REPO] <PR_ID>`).
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RequiredPositionals {
    pub(super) repo: Option<String>,
    /// One value for each required argument, in order.
    values: Vec<String>,
    /// The names of the required arguments, for error text.
    names: Vec<String>,
}

impl RequiredPositionals {
    /// `[REPO] <PR_ID>`: the argument list as the help shows it.
    fn usage(&self) -> String {
        std::iter::once("[REPO]".to_string())
            .chain(self.names.iter().map(|n| format!("<{n}>")))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `<PR_ID> <COMMENT_ID>`: the required arguments, for an example command.
    pub(super) fn required_usage(&self) -> String {
        self.names
            .iter()
            .map(|n| format!("<{n}>"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// How the values were read, when no repository came with them.
    ///
    /// `bb branch get web_app` outside a checkout has one value, and the count
    /// makes it the branch. The user most probably gave the repository and
    /// forgot the branch. An error about a missing repository alone would not
    /// show that.
    pub(super) fn read_note(&self) -> Option<String> {
        if self.repo.is_some() {
            return None;
        }
        let read = self
            .values
            .iter()
            .zip(&self.names)
            .map(|(value, name)| format!("'{value}' was read as <{name}>"))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("{read}. Arguments: {}.", self.usage()))
    }

    /// The required argument at `index`, as text.
    pub(super) fn text(&self, index: usize) -> &str {
        &self.values[index]
    }

    /// The required argument at `index`, as a number.
    ///
    /// A value that is not a number is most frequently a repository slug with
    /// no identifier after it, so the error shows the argument order.
    pub(super) fn number(&self, index: usize) -> Result<i64> {
        let value = &self.values[index];
        match value.parse::<i64>() {
            Ok(number) => Ok(number),
            Err(_) => bail!(
                "Invalid {} '{value}': expected a number. Arguments: {}.",
                self.names[index],
                self.usage()
            ),
        }
    }
}

/// Split `values` into an optional repository and the required arguments.
///
/// `required` names the arguments that follow the repository, in order
/// (`["PR_ID"]` for `pr get`). Each of them is necessary, so the count alone
/// decides: one value more than `required` means that the first value is the
/// repository. No guess about the shape of a value is necessary.
///
/// clap accepts an empty list, because a list that clap requires shows in the
/// help as `<REPO> [PR_ID]...`, which is the reverse of the truth. The check
/// for a missing value is thus here.
pub(super) fn split_repo_and_required(
    values: Vec<String>,
    required: &[&str],
) -> Result<RequiredPositionals> {
    let mut given = RequiredPositionals {
        repo: None,
        values: Vec::new(),
        names: required.iter().map(|n| n.to_string()).collect(),
    };
    let count = values.len();
    if count > required.len() + 1 {
        bail!(
            "Too many arguments: expected {}, got {count} values ({}).",
            given.usage(),
            values.join(" ")
        );
    }
    if count == 0 {
        bail!("Missing argument: expected {}.", given.usage());
    }
    if count < required.len() {
        bail!(
            "Missing argument: expected {}, got {count} values ({}).",
            given.usage(),
            values.join(" ")
        );
    }
    let mut values = values.into_iter();
    if count == required.len() + 1 {
        given.repo = values.next();
    }
    given.values = values.collect();
    tracing::debug!(
        positional_repo = ?given.repo,
        required = ?given.names,
        "Split the positional arguments"
    );
    Ok(given)
}

/// The repository to use: positional, then `--repo`, then the git remote.
///
/// A positional repository and a `--repo` that disagree are an error rather
/// than a silent choice, since either could be the one the user meant.
pub(super) fn choose_repo(
    positional: Option<&str>,
    flag: Option<&str>,
    git_detected: Option<&str>,
) -> Result<Option<String>> {
    match (positional, flag) {
        (Some(p), Some(f)) if p != f => {
            bail!("Repository given twice: '{p}' as an argument and '{f}' with --repo.")
        }
        (Some(p), _) => Ok(Some(p.to_string())),
        (None, Some(f)) => Ok(Some(f.to_string())),
        (None, None) => Ok(git_detected.map(String::from)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(values: &[&str], slots: &[&str], filled: &[bool]) -> Result<Positionals> {
        split_leading_repo(
            values.iter().map(|v| v.to_string()).collect(),
            slots,
            filled,
        )
    }

    fn ok(repo: Option<&str>, ids: &[&str]) -> Positionals {
        Positionals {
            repo: repo.map(String::from),
            ids: ids.iter().map(|v| v.to_string()).collect(),
        }
    }

    const UUID: &str = "{11111111-2222-3333-4444-555555555555}";

    #[test]
    fn identifiers_are_digits_or_uuids() {
        assert!(is_pipeline_identifier("592"));
        assert!(is_pipeline_identifier(UUID));
        assert!(is_pipeline_identifier(
            "11111111-2222-3333-4444-555555555555"
        ));
        assert!(!is_pipeline_identifier("web_app"));
        assert!(!is_pipeline_identifier("my-repo"));
        assert!(!is_pipeline_identifier("a-b-c-d-e"));
        assert!(!is_pipeline_identifier(""));
    }

    /// The reported case, and the forms that already worked.
    #[test]
    fn a_repo_leads_the_identifiers() {
        let slots = &["PIPELINE"];
        assert_eq!(
            split(&["web_app", "592"], slots, &[false]).unwrap(),
            ok(Some("web_app"), &["592"])
        );
        assert_eq!(
            split(&["592"], slots, &[false]).unwrap(),
            ok(None, &["592"])
        );
        assert_eq!(split(&[UUID], slots, &[false]).unwrap(), ok(None, &[UUID]));
        assert_eq!(
            split(&["web_app"], slots, &[false]).unwrap(),
            ok(Some("web_app"), &[])
        );
        assert_eq!(split(&[], slots, &[false]).unwrap(), ok(None, &[]));
    }

    #[test]
    fn commands_without_identifiers_take_only_a_repo() {
        assert_eq!(
            split(&["web_app"], &[], &[]).unwrap(),
            ok(Some("web_app"), &[])
        );
        assert!(split(&["web_app", "extra"], &[], &[]).is_err());
    }

    /// A numeric slug is a repository when the count says so.
    #[test]
    fn a_numeric_repo_is_recognised_by_count() {
        let slots = &["PIPELINE"];
        assert_eq!(
            split(&["123", "592"], slots, &[false]).unwrap(),
            ok(Some("123"), &["592"])
        );
    }

    #[test]
    fn logs_takes_a_repo_a_pipeline_and_a_step() {
        let slots = &["PIPELINE", "STEP_UUID"];
        let none = &[false, false];
        assert_eq!(
            split(&["592", UUID], slots, none).unwrap(),
            ok(None, &["592", UUID])
        );
        assert_eq!(
            split(&["web_app", "592"], slots, none).unwrap(),
            ok(Some("web_app"), &["592"])
        );
        assert_eq!(
            split(&["web_app", "592", UUID], slots, none).unwrap(),
            ok(Some("web_app"), &["592", UUID])
        );
        assert!(split(&["a", "b", "c", "d"], slots, none).is_err());
    }

    /// `--pipeline` fills its slot, so the positional that remains is the repo,
    /// or, for `logs`, the step.
    #[test]
    fn a_flag_filled_slot_shifts_the_positionals() {
        assert_eq!(
            split(&["web_app"], &["PIPELINE"], &[true]).unwrap(),
            ok(Some("web_app"), &[])
        );
        assert_eq!(
            split(&[UUID], &["PIPELINE", "STEP_UUID"], &[true, false]).unwrap(),
            ok(None, &[UUID])
        );
    }

    /// `get 592 --pipeline 593` was a clap conflict; it must not quietly become
    /// a repository called 592.
    #[test]
    fn an_identifier_given_twice_is_an_error() {
        let err = split(&["592"], &["PIPELINE"], &[true])
            .unwrap_err()
            .to_string();
        assert!(err.contains("--repo"), "{err}");
    }

    #[test]
    fn the_repo_comes_from_the_argument_the_flag_or_git() {
        assert_eq!(
            choose_repo(Some("a"), None, Some("g")).unwrap().as_deref(),
            Some("a")
        );
        assert_eq!(
            choose_repo(None, Some("f"), Some("g")).unwrap().as_deref(),
            Some("f")
        );
        assert_eq!(
            choose_repo(None, None, Some("g")).unwrap().as_deref(),
            Some("g")
        );
        assert_eq!(
            choose_repo(Some("a"), Some("a"), None).unwrap().as_deref(),
            Some("a")
        );
        assert_eq!(choose_repo(None, None, None).unwrap(), None);
        let err = choose_repo(Some("a"), Some("b"), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("'a'") && err.contains("'b'"), "{err}");
    }

    fn required(values: &[&str], names: &[&str]) -> Result<RequiredPositionals> {
        split_repo_and_required(values.iter().map(|v| v.to_string()).collect(), names)
    }

    #[test]
    fn required_arguments_are_split_by_count() {
        let with_repo = required(&["web_app", "42"], &["PR_ID"]).unwrap();
        assert_eq!(with_repo.repo.as_deref(), Some("web_app"));
        assert_eq!(with_repo.number(0).unwrap(), 42);

        let without_repo = required(&["42"], &["PR_ID"]).unwrap();
        assert_eq!(without_repo.repo, None);
        assert_eq!(without_repo.number(0).unwrap(), 42);

        // A branch name is not a repository, although it does not look like
        // an identifier.
        let branch = required(&["main"], &["BRANCH"]).unwrap();
        assert_eq!(branch.repo, None);
        assert_eq!(branch.text(0), "main");

        let two = required(&["web_app", "42", "9"], &["PR_ID", "COMMENT_ID"]).unwrap();
        assert_eq!(two.repo.as_deref(), Some("web_app"));
        assert_eq!((two.number(0).unwrap(), two.number(1).unwrap()), (42, 9));

        let two_no_repo = required(&["42", "9"], &["PR_ID", "COMMENT_ID"]).unwrap();
        assert_eq!(two_no_repo.repo, None);
        assert_eq!(two_no_repo.number(1).unwrap(), 9);
    }

    #[test]
    fn the_read_note_names_each_value_when_no_repo_came_with_them() {
        let one = required(&["web_app"], &["BRANCH"]).unwrap();
        assert_eq!(
            one.read_note().as_deref(),
            Some("'web_app' was read as <BRANCH>. Arguments: [REPO] <BRANCH>.")
        );
        let two = required(&["42", "9"], &["PR_ID", "COMMENT_ID"]).unwrap();
        assert_eq!(
            two.read_note().as_deref(),
            Some(
                "'42' was read as <PR_ID>, '9' was read as <COMMENT_ID>. \
                 Arguments: [REPO] <PR_ID> <COMMENT_ID>."
            )
        );
        assert_eq!(two.required_usage(), "<PR_ID> <COMMENT_ID>");
        let with_repo = required(&["web_app", "main"], &["BRANCH"]).unwrap();
        assert_eq!(with_repo.read_note(), None);
    }

    #[test]
    fn a_wrong_count_of_required_arguments_is_an_error() {
        let few = required(&["42"], &["PR_ID", "COMMENT_ID"])
            .unwrap_err()
            .to_string();
        assert!(
            few.contains("Missing argument") && few.contains("[REPO] <PR_ID> <COMMENT_ID>"),
            "{few}"
        );
        let many = required(&["a", "1", "2"], &["PR_ID"])
            .unwrap_err()
            .to_string();
        assert!(many.contains("Too many arguments"), "{many}");
    }

    #[test]
    fn a_value_that_is_not_a_number_names_the_argument_order() {
        let given = required(&["web_app"], &["PR_ID"]).unwrap();
        assert_eq!(
            given.number(0).unwrap_err().to_string(),
            "Invalid PR_ID 'web_app': expected a number. Arguments: [REPO] <PR_ID>."
        );
    }

    /// The whole path from the command line: clap accepts the forms, and the
    /// split gives the repository and identifiers the dispatcher uses.
    mod parsing {
        use super::super::super::{BitbucketArgs, BitbucketCommands, PipelineCommands, PrCommands};
        use super::super::{split_leading_repo, split_repo_and_required};
        use clap::Parser;

        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            args: BitbucketArgs,
        }

        fn parse(argv: &[&str]) -> BitbucketArgs {
            Cli::try_parse_from(std::iter::once("bb").chain(argv.iter().copied()))
                .unwrap_or_else(|e| panic!("{argv:?}: {e}"))
                .args
        }

        fn pipeline(argv: &[&str]) -> PipelineCommands {
            match parse(argv).command {
                BitbucketCommands::Pipeline(cmd) => cmd,
                other => panic!("not a pipeline command: {other:?}"),
            }
        }

        #[test]
        fn pipeline_list_takes_the_repo_positionally() {
            match pipeline(&["pipeline", "list", "web_app", "--limit", "5"]) {
                PipelineCommands::List {
                    repo_arg, limit, ..
                } => {
                    assert_eq!(repo_arg.as_deref(), Some("web_app"));
                    assert_eq!(limit, 5);
                }
                other => panic!("{other:?}"),
            }
        }

        #[test]
        fn pipeline_get_takes_a_repo_and_a_build_number() {
            let PipelineCommands::Get {
                targets,
                pipeline_flag,
                ..
            } = pipeline(&["pipeline", "get", "web_app", "592", "--steps"])
            else {
                panic!("not get");
            };
            let given =
                split_leading_repo(targets, &["PIPELINE"], &[pipeline_flag.is_some()]).unwrap();
            assert_eq!(given.repo.as_deref(), Some("web_app"));
            assert_eq!(given.ids, vec!["592".to_string()]);
        }

        /// What already worked keeps working: an id alone, and --repo either
        /// side of the subcommand.
        #[test]
        fn the_existing_forms_still_parse() {
            for argv in [
                &["pipeline", "get", "592"][..],
                &["--repo", "web_app", "pipeline", "get", "592"],
                &["pipeline", "get", "592", "--repo", "web_app"],
                &["pipeline", "get", "--pipeline", "592"],
                &["pipeline", "steps", "592"],
                &["pipeline", "status", "--wait"],
            ] {
                let args = parse(argv);
                if argv.contains(&"--repo") {
                    assert_eq!(args.repo.as_deref(), Some("web_app"), "{argv:?}");
                }
            }
        }

        #[test]
        fn pipeline_logs_takes_a_repo_a_build_and_a_step() {
            let PipelineCommands::Logs { targets, .. } = pipeline(&[
                "pipeline",
                "logs",
                "web_app",
                "592",
                "{s-1}",
                "--failed-only",
            ]) else {
                panic!("not logs");
            };
            let given =
                split_leading_repo(targets, &["PIPELINE", "STEP_UUID"], &[false, false]).unwrap();
            assert_eq!(given.repo.as_deref(), Some("web_app"));
            assert_eq!(given.ids, vec!["592".to_string(), "{s-1}".to_string()]);
        }

        #[test]
        fn rerun_with_pr_takes_only_a_repo() {
            let PipelineCommands::Rerun { targets, pr, .. } =
                pipeline(&["pipeline", "rerun", "web_app", "--pr", "7"])
            else {
                panic!("not rerun");
            };
            let given = split_leading_repo(targets, &["PIPELINE"], &[pr.is_some()]).unwrap();
            assert_eq!(given.repo.as_deref(), Some("web_app"));
            assert!(given.ids.is_empty());
        }

        fn pr_get(argv: &[&str]) -> (Option<String>, Vec<String>) {
            let args = parse(argv);
            match args.command {
                BitbucketCommands::Pr(PrCommands::Get { targets }) => (args.repo, targets),
                other => panic!("not pr get: {other:?}"),
            }
        }

        fn strings(values: &[&str]) -> Vec<String> {
            values.iter().map(|v| v.to_string()).collect()
        }

        /// The command that failed with "unexpected argument '--repo'".
        #[test]
        fn pr_get_takes_repo_as_a_flag_before_the_id() {
            let (flag, targets) = pr_get(&["pr", "get", "--repo", "web_app", "42"]);
            assert_eq!(flag.as_deref(), Some("web_app"));
            assert_eq!(targets, strings(&["42"]));
            let given = split_repo_and_required(targets, &["PR_ID"]).unwrap();
            assert_eq!(given.repo, None);
            assert_eq!(given.number(0).unwrap(), 42);
        }

        #[test]
        fn pr_get_takes_repo_as_a_flag_in_other_positions() {
            for argv in [
                &["--repo", "web_app", "pr", "get", "42"][..],
                &["pr", "--repo", "web_app", "get", "42"],
                &["pr", "get", "42", "--repo", "web_app"],
                &["pr", "get", "--repo=web_app", "42"],
            ] {
                let (flag, targets) = pr_get(argv);
                assert_eq!(flag.as_deref(), Some("web_app"), "{argv:?}");
                assert_eq!(targets, strings(&["42"]), "{argv:?}");
            }
        }

        /// In a checkout: one value, no flag. The Git remote gives the repository.
        #[test]
        fn pr_get_takes_an_id_alone() {
            let (flag, targets) = pr_get(&["pr", "get", "42"]);
            assert_eq!(flag, None);
            let given = split_repo_and_required(targets, &["PR_ID"]).unwrap();
            assert_eq!(given.repo, None);
            assert_eq!(given.number(0).unwrap(), 42);
        }

        /// The form that worked before this change.
        #[test]
        fn pr_get_takes_a_repo_and_an_id() {
            let (flag, targets) = pr_get(&["pr", "get", "web_app", "42"]);
            assert_eq!(flag, None);
            let given = split_repo_and_required(targets, &["PR_ID"]).unwrap();
            assert_eq!(given.repo.as_deref(), Some("web_app"));
            assert_eq!(given.number(0).unwrap(), 42);
        }

        #[test]
        fn pr_get_rejects_no_value_and_three_values() {
            assert!(Cli::try_parse_from(["bb", "pr", "get", "web_app", "42", "43"]).is_err());
            for argv in [&["pr", "get"][..], &["pr", "get", "--repo", "web_app"]] {
                let (_, targets) = pr_get(argv);
                let error = split_repo_and_required(targets, &["PR_ID"])
                    .unwrap_err()
                    .to_string();
                assert_eq!(error, "Missing argument: expected [REPO] <PR_ID>.");
            }
        }

        /// Options after the list do not become part of it.
        #[test]
        fn pr_comment_takes_the_list_and_then_its_options() {
            let args = parse(&["pr", "comment", "--repo", "web_app", "42", "--text", "LGTM"]);
            let BitbucketCommands::Pr(PrCommands::Comment { targets, text, .. }) = args.command
            else {
                panic!("not pr comment");
            };
            assert_eq!(targets, strings(&["42"]));
            assert_eq!(text, "LGTM");
        }

        /// `--unresolved` came with pull request 183, before the argument
        /// tree moved. It must work with each form of the repository.
        #[test]
        fn pr_comments_takes_unresolved_with_each_repository_form() {
            for (argv, expected) in [
                (
                    &["pr", "comments", "web_app", "42", "--unresolved"][..],
                    true,
                ),
                (
                    &["pr", "comments", "--repo", "web_app", "42", "--unresolved"],
                    true,
                ),
                (&["pr", "comments", "--unresolved", "42"], true),
                (&["pr", "comments", "web_app", "42"], false),
            ] {
                let args = parse(argv);
                let BitbucketCommands::Pr(PrCommands::Comments {
                    targets,
                    unresolved,
                }) = args.command
                else {
                    panic!("not pr comments: {argv:?}");
                };
                assert_eq!(unresolved, expected, "{argv:?}");
                assert_eq!(targets.last().map(String::as_str), Some("42"), "{argv:?}");
            }
        }

        #[test]
        fn more_values_than_the_command_takes_are_rejected() {
            let result = Cli::try_parse_from(["bb", "pipeline", "get", "a", "b", "c"]);
            assert!(result.is_err());
        }
    }
}
