/// Builder for constructing JQL (Jira Query Language) queries from filter parameters
pub struct JqlBuilder {
    conditions: Vec<String>,
}

impl JqlBuilder {
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }

    /// Add an equality condition (field = value)
    pub fn eq(mut self, field: &str, value: &str) -> Self {
        let normalized = Self::normalize_value(field, value);
        self.conditions.push(format!("{} = {}", field, normalized));
        self
    }

    /// Add an IN condition for multiple values (field IN (val1, val2, ...))
    pub fn in_list(mut self, field: &str, values: &[String]) -> Self {
        if values.is_empty() {
            return self;
        }

        let escaped_values: Vec<String> =
            values.iter().map(|v| Self::escape_and_quote(v)).collect();

        self.conditions
            .push(format!("{} IN ({})", field, escaped_values.join(", ")));
        self
    }

    /// Add a text search condition (field ~ "value")
    pub fn contains(mut self, field: &str, value: &str) -> Self {
        let escaped = Self::escape_and_quote(value);
        self.conditions.push(format!("{} ~ {}", field, escaped));
        self
    }

    /// Build the final JQL query string
    pub fn finish(self) -> String {
        if self.conditions.is_empty() {
            return String::new();
        }
        self.conditions.join(" AND ")
    }

    /// Escape and quote a value
    fn escape_and_quote(value: &str) -> String {
        let escaped = value
            .replace('\\', "\\\\") // Escape backslashes first
            .replace('"', "\\\""); // Then escape quotes
        format!("\"{}\"", escaped)
    }

    /// Normalize special values based on field context
    fn normalize_value(field: &str, value: &str) -> String {
        match (field, value) {
            // Handle @me shorthand for user fields
            ("assignee" | "reporter" | "creator" | "watcher", "@me") => "currentUser()".to_string(),
            // Handle unassigned/empty shorthand
            (_, "unassigned" | "none" | "empty") => "EMPTY".to_string(),
            // Default: escape and quote
            _ => Self::escape_and_quote(value),
        }
    }
}

impl Default for JqlBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_builder() {
        let query = JqlBuilder::new().finish();
        assert_eq!(query, "");
    }

    #[test]
    fn test_single_eq() {
        let query = JqlBuilder::new().eq("project", "PROJ").finish();
        assert_eq!(query, "project = \"PROJ\"");
    }

    #[test]
    fn test_assignee_me_shorthand() {
        let query = JqlBuilder::new().eq("assignee", "@me").finish();
        assert_eq!(query, "assignee = currentUser()");
    }

    #[test]
    fn test_reporter_me_shorthand() {
        let query = JqlBuilder::new().eq("reporter", "@me").finish();
        assert_eq!(query, "reporter = currentUser()");
    }

    #[test]
    fn test_unassigned_shorthand() {
        let query = JqlBuilder::new().eq("assignee", "unassigned").finish();
        assert_eq!(query, "assignee = EMPTY");
    }

    #[test]
    fn test_multiple_conditions() {
        let query = JqlBuilder::new()
            .eq("assignee", "@me")
            .eq("project", "TEST")
            .finish();
        assert_eq!(query, "assignee = currentUser() AND project = \"TEST\"");
    }

    #[test]
    fn test_in_list_single() {
        let query = JqlBuilder::new()
            .in_list("status", &[String::from("Open")])
            .finish();
        assert_eq!(query, "status IN (\"Open\")");
    }

    #[test]
    fn test_in_list_multiple() {
        let query = JqlBuilder::new()
            .in_list(
                "status",
                &[String::from("Open"), String::from("In Progress")],
            )
            .finish();
        assert_eq!(query, "status IN (\"Open\", \"In Progress\")");
    }

    #[test]
    fn test_in_list_empty() {
        let query = JqlBuilder::new().in_list("status", &[]).finish();
        assert_eq!(query, "");
    }

    #[test]
    fn test_contains() {
        let query = JqlBuilder::new().contains("summary", "bug fix").finish();
        assert_eq!(query, "summary ~ \"bug fix\"");
    }

    #[test]
    fn test_quote_escape() {
        let query = JqlBuilder::new()
            .eq("summary", "Fix \"bug\" issue")
            .finish();
        assert_eq!(query, "summary = \"Fix \\\"bug\\\" issue\"");
    }

    #[test]
    fn test_backslash_escape() {
        let query = JqlBuilder::new().eq("summary", "Path\\to\\file").finish();
        assert_eq!(query, "summary = \"Path\\\\to\\\\file\"");
    }

    #[test]
    fn test_complex_query() {
        let query = JqlBuilder::new()
            .eq("assignee", "@me")
            .in_list(
                "status",
                &[String::from("Open"), String::from("In Progress")],
            )
            .eq("priority", "High")
            .in_list("label", &[String::from("bug"), String::from("backend")])
            .finish();

        assert_eq!(
            query,
            "assignee = currentUser() AND status IN (\"Open\", \"In Progress\") AND priority = \"High\" AND label IN (\"bug\", \"backend\")"
        );
    }
}
