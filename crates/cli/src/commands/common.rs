use anyhow::Result;
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

#[cfg(test)]
mod tests {
    use super::*;

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
