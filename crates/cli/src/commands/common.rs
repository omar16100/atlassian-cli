use anyhow::{Context, Result};
use atlassian_cli_output::{OutputFormat, OutputRenderer};
use serde::Serialize;

/// Result struct for successful mutations (create, update, delete, etc.)
#[derive(Serialize)]
pub struct MutationResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl MutationResult {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            id: None,
        }
    }

    pub fn with_id(message: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            id: Some(id.into()),
        }
    }
}

/// Render a success message respecting the output format.
/// For Table format: prints emoji message to stdout
/// For other formats: renders structured JSON/YAML/CSV/Quiet
pub fn render_success(
    renderer: &OutputRenderer,
    emoji_message: &str,
    result: &MutationResult,
) -> Result<()> {
    match renderer.format() {
        OutputFormat::Table | OutputFormat::Markdown => {
            println!("{emoji_message}");
            Ok(())
        }
        OutputFormat::Quiet => {
            if let Some(id) = &result.id {
                println!("{id}");
            }
            Ok(())
        }
        _ => renderer.render(&result),
    }
}

/// File format written by the `export` commands (`jira bulk export`,
/// `jira audit export`, `confluence bulk export`).
///
/// Chosen with `--export-format`. It used to be a `--format` of their own,
/// which reused the id of the global `--format` (`OutputFormat`) and made clap
/// panic on every run of those commands, so it has to have a different name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ExportFormat {
    Json,
    Csv,
}

/// Pick the file format for an export.
///
/// `--export-format` wins. Without it, the global `--format` decides when it
/// names a format an export can write (json or csv), so the documented
/// `--output issues.csv --format csv` writes CSV rather than silently writing
/// JSON into a `.csv` file. Anything else there (the default table, yaml,
/// markdown, quiet) only shapes what is printed to stdout, and the file is
/// JSON, as it always defaulted to.
pub fn resolve_export_format(explicit: Option<ExportFormat>, global: OutputFormat) -> ExportFormat {
    let (chosen, source) = match (explicit, global) {
        (Some(format), _) => (format, "--export-format"),
        (None, OutputFormat::Csv) => (ExportFormat::Csv, "--format"),
        (None, OutputFormat::Json) => (ExportFormat::Json, "--format"),
        (None, _) => (ExportFormat::Json, "default"),
    };
    tracing::debug!(?chosen, source, ?global, "resolved export file format");
    chosen
}

/// Decide whether a typed confirmation matches what the command demanded.
///
/// Split out from the prompting so the comparison is testable without a
/// terminal. Whitespace is trimmed because a pasted repository slug often
/// carries a trailing space, but the match is otherwise exact and
/// case-sensitive: "yes" must not stand in for the resource's own name.
pub fn confirmation_matches(expected: &str, typed: &str) -> bool {
    !expected.is_empty() && typed.trim() == expected
}

/// Require the user to type `expected` before a destructive operation runs.
///
/// Modelled on `resolve_value` in `auth.rs`: the prompt goes to stderr so
/// stdout stays redirectable, and the absence of a terminal is a hard error
/// rather than a hang. That matters more here than at login, because the
/// alternative to failing loudly is deleting someone's branches from a cron
/// job that never had a chance to answer.
pub fn confirm_destructive(expected: &str, warning: &str) -> Result<()> {
    confirm_destructive_on(
        expected,
        warning,
        std::io::IsTerminal::is_terminal(&std::io::stdin()),
    )
}

/// The body of [`confirm_destructive`], with the terminal check as a parameter.
///
/// Split out because a test cannot control whether `cargo test` inherits a
/// terminal on stdin. A test asserting the no-terminal refusal passed under a
/// pipe and failed under a pseudo-terminal, which made its result a property of
/// the runner rather than of the code.
pub(crate) fn confirm_destructive_on(
    expected: &str,
    warning: &str,
    stdin_is_terminal: bool,
) -> Result<()> {
    use std::io::Write;

    if !stdin_is_terminal {
        anyhow::bail!(
            "{warning}\nRefusing to continue: no terminal available to confirm. \
             Pass --yes to skip this prompt in a script."
        );
    }

    eprintln!("{warning}");
    eprint!("Type '{expected}' to confirm: ");
    std::io::stderr()
        .flush()
        .context("Failed to write confirmation prompt")?;

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("Failed to read confirmation")?;

    if !confirmation_matches(expected, &line) {
        anyhow::bail!("Confirmation did not match '{expected}'. Nothing was changed.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_format_flag_wins_over_the_global_format() {
        assert_eq!(
            resolve_export_format(Some(ExportFormat::Csv), OutputFormat::Json),
            ExportFormat::Csv
        );
        assert_eq!(
            resolve_export_format(Some(ExportFormat::Json), OutputFormat::Csv),
            ExportFormat::Json
        );
    }

    /// `--format csv` and `--format json` are what the README and the example
    /// scripts have always written, so without `--export-format` they pick the
    /// file format.
    #[test]
    fn global_json_or_csv_picks_the_file_format_when_not_overridden() {
        assert_eq!(
            resolve_export_format(None, OutputFormat::Csv),
            ExportFormat::Csv
        );
        assert_eq!(
            resolve_export_format(None, OutputFormat::Json),
            ExportFormat::Json
        );
    }

    /// Output formats with no file equivalent keep the old JSON default.
    #[test]
    fn other_global_formats_fall_back_to_json() {
        for global in [
            OutputFormat::Table,
            OutputFormat::Yaml,
            OutputFormat::Markdown,
            OutputFormat::Quiet,
        ] {
            assert_eq!(
                resolve_export_format(None, global),
                ExportFormat::Json,
                "{global:?}"
            );
        }
    }

    /// Without a terminal the prompt must refuse, not proceed and not hang. A
    /// scheduled job that never had a chance to answer must not delete.
    #[test]
    fn without_a_terminal_it_refuses() {
        let err = confirm_destructive_on("repo", "about to delete", false)
            .expect_err("must refuse when nothing can answer");
        let message = format!("{err:#}");
        assert!(message.contains("Refusing to continue"), "{message}");
        assert!(
            message.contains("--yes"),
            "must say how to proceed: {message}"
        );
    }

    #[test]
    fn confirmation_requires_the_exact_resource_name() {
        assert!(confirmation_matches("my-repo", "my-repo"));
        // Trailing whitespace from a paste is forgiven.
        assert!(confirmation_matches("my-repo", "  my-repo \n"));
        // A generic affirmative is not a substitute for naming the resource.
        assert!(!confirmation_matches("my-repo", "yes"));
        assert!(!confirmation_matches("my-repo", "y"));
        assert!(!confirmation_matches("my-repo", ""));
        // Case-sensitive, and no partial matches.
        assert!(!confirmation_matches("my-repo", "My-Repo"));
        assert!(!confirmation_matches("my-repo", "my-repo-2"));
    }

    /// An empty expectation must never auto-confirm, or a caller that forgot to
    /// supply a name would silently skip the gate entirely.
    #[test]
    fn empty_expectation_never_confirms() {
        assert!(!confirmation_matches("", ""));
        assert!(!confirmation_matches("", "anything"));
    }

    #[test]
    fn test_mutation_result_new() {
        let result = MutationResult::new("Created issue");
        assert!(result.success);
        assert_eq!(result.message, "Created issue");
        assert!(result.id.is_none());
    }

    #[test]
    fn test_mutation_result_with_id() {
        let result = MutationResult::with_id("Created issue", "PROJ-123");
        assert!(result.success);
        assert_eq!(result.message, "Created issue");
        assert_eq!(result.id, Some("PROJ-123".to_string()));
    }

    #[test]
    fn test_render_success_table() {
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let result = MutationResult::with_id("Created", "123");
        // Table format should just print the emoji message
        assert!(render_success(&renderer, "✅ Created", &result).is_ok());
    }

    #[test]
    fn test_render_success_json() {
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let result = MutationResult::with_id("Created", "123");
        assert!(render_success(&renderer, "✅ Created", &result).is_ok());
    }

    #[test]
    fn test_render_success_quiet() {
        let renderer = OutputRenderer::new(OutputFormat::Quiet);
        let result = MutationResult::with_id("Created", "123");
        assert!(render_success(&renderer, "✅ Created", &result).is_ok());
    }

    #[test]
    fn test_render_success_quiet_no_id() {
        let renderer = OutputRenderer::new(OutputFormat::Quiet);
        let result = MutationResult::new("Deleted");
        assert!(render_success(&renderer, "✅ Deleted", &result).is_ok());
    }

    #[test]
    fn test_render_success_markdown() {
        let renderer = OutputRenderer::new(OutputFormat::Markdown);
        let result = MutationResult::with_id("Created", "123");
        assert!(render_success(&renderer, "✅ Created", &result).is_ok());
    }
}
