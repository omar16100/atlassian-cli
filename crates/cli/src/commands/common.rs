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
    use std::io::{IsTerminal, Write};

    if !std::io::stdin().is_terminal() {
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
