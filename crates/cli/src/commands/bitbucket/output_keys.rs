//! The keys of the `-f json` and `-f yaml` output of the pull request and
//! pipeline commands, as `--help` prints them.
//!
//! The rows of these commands are flat: `source`, `destination`, `author` and
//! `state` are strings. The Bitbucket REST API returns objects for the same
//! names, and scripts that expected that shape failed on the first `.get()`.
//! No help text said which keys exist, so callers went to `bb api` for data
//! that the commands already return.
//!
//! Each table below is the single description of one output. The long help of
//! the command is built from it, and tests compare it with the keys that the
//! row types serialise, so the help cannot go out of date without a failure.

/// One key of an output object.
pub(super) struct Key {
    pub(super) name: &'static str,
    /// The JSON type, as a reader of the help needs it.
    pub(super) kind: &'static str,
    pub(super) note: &'static str,
    /// Not in each object. The help marks the key with `*`.
    pub(super) optional: bool,
}

const fn key(name: &'static str, kind: &'static str, note: &'static str) -> Key {
    Key {
        name,
        kind,
        note,
        optional: false,
    }
}

const fn optional(name: &'static str, kind: &'static str, note: &'static str) -> Key {
    Key {
        name,
        kind,
        note,
        optional: true,
    }
}

pub(super) const PR_LIST: &[Key] = &[
    key("id", "number", "Pull request ID"),
    key("title", "string", ""),
    key("state", "string", "OPEN, MERGED, DECLINED or SUPERSEDED"),
    key("author", "string", "Display name (not an object)"),
    key("source", "string", "Source branch name (not an object)"),
    key(
        "destination",
        "string",
        "Destination branch name (not an object)",
    ),
];

pub(super) const PR_GET: &[Key] = &[
    key("id", "number", "Pull request ID"),
    key("title", "string", ""),
    key("state", "string", "OPEN, MERGED, DECLINED or SUPERSEDED"),
    key("author", "string", "Display name (not an object)"),
    key("source", "string", "Source branch name (not an object)"),
    key(
        "destination",
        "string",
        "Destination branch name (not an object)",
    ),
    key("description", "string", "Empty when there is none"),
    key("created", "string", "Timestamp, empty when not reported"),
    key("updated", "string", "Timestamp, empty when not reported"),
    key("comments", "string", "Comment count as text"),
    key("tasks", "string", "Task count as text"),
    key("approvals", "string", "Count of approvals as text"),
    optional("reviewers", "list", "Absent when there are no reviewers"),
];

/// One entry of `reviewers` in `pr get`.
pub(super) const PR_REVIEWER: &[Key] = &[
    key("name", "string", "Display name"),
    key("uuid", "string", "The value that --reviewers accepts"),
    key("role", "string", "REVIEWER"),
    key(
        "status",
        "string",
        "Approved, Changes Requested or No Response",
    ),
    key(
        "participated_on",
        "string",
        "Timestamp, empty before a response",
    ),
];

pub(super) const PIPELINE_LIST: &[Key] = &[
    key("build_number", "number|null", ""),
    key("state", "string", "Such as SUCCESSFUL, FAILED, PAUSED"),
    key("ref_name", "string|null", "Branch or tag name"),
    key("commit", "string|null", "First 7 characters of the hash"),
    key("target_type", "string|null", "Such as pipeline_ref_target"),
    key("created", "string|null", "Timestamp"),
    optional(
        "steps_summary",
        "string",
        "With --steps: `name STATUS | ...`",
    ),
    optional(
        "pending_manual_steps",
        "number",
        "With --steps, for a paused build",
    ),
    optional("commit_hash", "string", "Full hash. JSON and YAML only"),
];

pub(super) const PIPELINE_GET: &[Key] = &[
    key("uuid", "string", "Pipeline UUID, with braces"),
    key("build_number", "number|null", ""),
    key("state", "string", "Such as SUCCESSFUL, FAILED, PAUSED"),
    key("ref_name", "string|null", "Branch or tag name"),
    key("commit", "string|null", "First 7 characters of the hash"),
    key("created", "string|null", "Timestamp"),
    key("completed", "string|null", "Timestamp, null while it runs"),
    optional("steps", "list", "With --steps: see `pipeline steps`"),
    optional(
        "steps_summary",
        "string",
        "With --steps: `name STATUS | ...`",
    ),
    optional("pending_manual_steps", "number", "For a paused build"),
    optional("commit_hash", "string", "Full hash. JSON and YAML only"),
];

pub(super) const PIPELINE_STEP: &[Key] = &[
    key("uuid", "string", "The step UUID for `pipeline logs`"),
    key("name", "string", "The UUID when the step has no name"),
    key("status", "string", "Such as SUCCESSFUL, FAILED, PENDING"),
    optional("started", "string", "Timestamp"),
    optional("completed", "string", "Timestamp"),
    optional("duration", "string", "Such as `2m 5s`"),
    optional("logs_url", "string", "Not in table and markdown output"),
    optional("trigger", "string", "MANUAL or AUTOMATIC"),
];

/// The widest line that the help can have. clap wraps a longer line, and a
/// wrapped line breaks the columns.
#[cfg(test)]
const MAX_HELP_LINE: usize = 80;

/// The key table as help text: one aligned line for each key. A `*` marks a
/// key that is not in each object, and a line below the table says so.
fn key_lines(keys: &[Key]) -> String {
    let label = |k: &Key| format!("{}{}", k.name, if k.optional { "*" } else { "" });
    let name_width = keys.iter().map(|k| label(k).len()).max().unwrap_or(0);
    let kind_width = keys.iter().map(|k| k.kind.len()).max().unwrap_or(0);
    let mut lines: Vec<String> = keys
        .iter()
        .map(|k| {
            format!(
                "  {:name_width$}  {:kind_width$}  {}",
                label(k),
                k.kind,
                k.note
            )
            .trim_end()
            .to_string()
        })
        .collect();
    if keys.iter().any(|k| k.optional) {
        lines.push("  * Not in each object.".to_string());
    }
    lines.join("\n")
}

/// The closing paragraph: these rows are not the REST shape, and how to get it.
fn raw_shape_note(api_path: &str) -> String {
    format!(
        "The output is flat. It does not have the shape of the Bitbucket REST API.\n\
         For the API response, unchanged, use:\n  bitbucket api {api_path}"
    )
}

const LIST_NOTE: &str = "With --envelope, the list is in `data` and its length in `count`.";

pub(super) fn pr_list_help() -> String {
    format!(
        "List pull requests for a repository.\n\n\
         Output keys (-f json, -f yaml), one object for each pull request:\n{}\n\n{LIST_NOTE}\n\n{}",
        key_lines(PR_LIST),
        raw_shape_note("/2.0/repositories/<workspace>/<repo>/pullrequests")
    )
}

pub(super) fn pr_get_help() -> String {
    format!(
        "Get pull request details.\n\n\
         Output keys (-f json, -f yaml):\n{}\n\n\
         Keys of each `reviewers` entry:\n{}\n\n{}",
        key_lines(PR_GET),
        key_lines(PR_REVIEWER),
        raw_shape_note("/2.0/repositories/<workspace>/<repo>/pullrequests/<id>")
    )
}

pub(super) fn pipeline_list_help() -> String {
    format!(
        "List pipelines.\n\n\
         Output keys (-f json, -f yaml), one object for each pipeline:\n{}\n\n{LIST_NOTE}\n\n{}",
        key_lines(PIPELINE_LIST),
        raw_shape_note("/2.0/repositories/<workspace>/<repo>/pipelines/")
    )
}

pub(super) fn pipeline_get_help() -> String {
    format!(
        "Get pipeline details.\n\n\
         Output keys (-f json, -f yaml):\n{}\n\n{}",
        key_lines(PIPELINE_GET),
        raw_shape_note("/2.0/repositories/<workspace>/<repo>/pipelines/<uuid>")
    )
}

pub(super) fn pipeline_steps_help() -> String {
    format!(
        "List steps for a pipeline.\n\n\
         Output keys (-f json, -f yaml), one object for each step:\n{}\n\n{LIST_NOTE}\n\n{}",
        key_lines(PIPELINE_STEP),
        raw_shape_note("/2.0/repositories/<workspace>/<repo>/pipelines/<uuid>/steps/")
    )
}

/// Assert that `value` (one serialised output object) has exactly the keys
/// that `keys` documents. With `full`, each optional key must be there too.
#[cfg(test)]
pub(super) fn assert_documented(value: &serde_json::Value, keys: &[Key], full: bool, what: &str) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{what}: not an object: {value}"));
    for name in object.keys() {
        assert!(
            keys.iter().any(|k| k.name == name),
            "{what}: the output has the key `{name}`, and the help does not list it"
        );
    }
    for k in keys {
        if !k.optional || full {
            assert!(
                object.contains_key(k.name),
                "{what}: the help lists `{}`, and the output does not have it: {value}",
                k.name
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::pullrequests::{pr_list_row, pr_view, Participant, PullRequest};
    use atlassian_cli_output::OutputFormat;
    use serde_json::json;

    fn pull_request(reviewed: bool) -> (PullRequest, Vec<Participant>) {
        let participants = if reviewed {
            json!([{
                "approved": true,
                "state": "approved",
                "role": "REVIEWER",
                "participated_on": "2026-11-20T10:00:00Z",
                "user": {"display_name": "Second Dev", "uuid": "{u-2}"}
            }])
        } else {
            json!([])
        };
        let pr: PullRequest = serde_json::from_value(json!({
            "id": 42,
            "title": "Add the widget",
            "state": "OPEN",
            "author": {"display_name": "Dev User", "uuid": "{u-1}"},
            "source": {"branch": {"name": "feature/widget"}},
            "destination": {"branch": {"name": "main"}},
            "description": "Example",
            "created_on": "2026-11-19T10:00:00Z",
            "updated_on": "2026-11-20T10:00:00Z",
            "comment_count": 3,
            "task_count": 1,
            "participants": participants.clone()
        }))
        .unwrap();
        (pr, serde_json::from_value(participants).unwrap())
    }

    /// The reported failure: scripts read `source` as an object. The help now
    /// says that it is a string, and this keeps the help equal to the output.
    #[test]
    fn pr_list_help_lists_exactly_the_keys_of_a_row() {
        let (pr, _) = pull_request(false);
        let row = serde_json::to_value(pr_list_row(&pr)).unwrap();
        assert_documented(&row, PR_LIST, true, "pr list");
        assert_eq!(row["source"], "feature/widget", "a string, not an object");
        assert_eq!(row["author"], "Dev User");
    }

    #[test]
    fn pr_get_help_lists_exactly_the_keys_of_the_view() {
        let (pr, participants) = pull_request(true);
        let view = serde_json::to_value(pr_view(&pr, &participants, OutputFormat::Json)).unwrap();
        assert_documented(&view, PR_GET, true, "pr get");
        assert_documented(&view["reviewers"][0], PR_REVIEWER, true, "pr get reviewers");
        assert_eq!(view["comments"], "3", "a count as text");

        // `reviewers` is the one optional key: absent when the list is empty.
        let (pr, participants) = pull_request(false);
        let view = serde_json::to_value(pr_view(&pr, &participants, OutputFormat::Json)).unwrap();
        assert_documented(&view, PR_GET, false, "pr get with no reviewers");
        assert!(view.get("reviewers").is_none(), "{view}");
    }

    #[test]
    fn key_lines_are_aligned_and_have_no_trailing_space() {
        let text = key_lines(PR_LIST);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), PR_LIST.len());
        assert_eq!(lines[0], "  id           number  Pull request ID");
        // No note: the line ends with the type.
        assert_eq!(lines[1], "  title        string");
        assert!(lines.iter().all(|l| l.trim_end() == *l));
    }

    #[test]
    fn optional_keys_have_a_mark_and_a_legend() {
        let text = key_lines(PR_GET);
        assert!(text.contains("\n  reviewers*  "), "{text}");
        assert!(text.ends_with("\n  * Not in each object."), "{text}");
        assert!(!key_lines(PR_LIST).contains('*'));
    }

    /// clap wraps a line that is wider than the terminal, and that breaks the
    /// columns of the key table.
    #[test]
    fn no_help_line_is_wider_than_the_limit() {
        for help in [
            pr_list_help(),
            pr_get_help(),
            pipeline_list_help(),
            pipeline_get_help(),
            pipeline_steps_help(),
        ] {
            for line in help.lines() {
                assert!(
                    line.chars().count() <= MAX_HELP_LINE,
                    "{} characters: {line}",
                    line.chars().count()
                );
            }
        }
    }

    #[test]
    fn each_help_text_lists_each_key_of_its_table() {
        for (help, tables) in [
            (pr_list_help(), vec![PR_LIST]),
            (pr_get_help(), vec![PR_GET, PR_REVIEWER]),
            (pipeline_list_help(), vec![PIPELINE_LIST]),
            (pipeline_get_help(), vec![PIPELINE_GET]),
            (pipeline_steps_help(), vec![PIPELINE_STEP]),
        ] {
            for table in tables {
                for k in table {
                    let mark = if k.optional { "*" } else { "" };
                    assert!(
                        help.contains(&format!("\n  {}{mark} ", k.name)),
                        "`{}` is not in:\n{help}",
                        k.name
                    );
                }
            }
            assert!(help.contains("bitbucket api /2.0/repositories/"), "{help}");
        }
    }
}
