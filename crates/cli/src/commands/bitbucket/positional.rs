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

    /// The whole path from the command line: clap accepts the forms, and the
    /// split gives the repository and identifiers the dispatcher uses.
    mod parsing {
        use super::super::super::{BitbucketArgs, BitbucketCommands, PipelineCommands};
        use super::super::split_leading_repo;
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

        #[test]
        fn more_values_than_the_command_takes_are_rejected() {
            let result = Cli::try_parse_from(["bb", "pipeline", "get", "a", "b", "c"]);
            assert!(result.is_err());
        }
    }
}
