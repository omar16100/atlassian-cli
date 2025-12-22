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

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Property: Any string can be safely escaped without panicking
            #[test]
            fn escape_never_panics(input in "\\PC*") {
                let _ = JqlBuilder::escape_and_quote(&input);
            }

            /// Property: Escaped strings always start and end with quotes
            #[test]
            fn escaped_strings_are_quoted(input in "\\PC*") {
                let escaped = JqlBuilder::escape_and_quote(&input);
                prop_assert!(escaped.starts_with('"'));
                prop_assert!(escaped.ends_with('"'));
            }

            /// Property: Escaping preserves length relationships
            #[test]
            fn escaping_increases_or_maintains_length(input in "\\PC*") {
                let escaped = JqlBuilder::escape_and_quote(&input);
                // Escaped length should be >= original + 2 (for quotes)
                prop_assert!(escaped.len() >= input.len() + 2);
            }

            /// Property: No unescaped quotes in escaped output (except surrounding quotes)
            #[test]
            fn no_unescaped_quotes_in_output(input in "\\PC*") {
                let escaped = JqlBuilder::escape_and_quote(&input);
                let inner = &escaped[1..escaped.len()-1];
                // Check that any quote in the inner string is escaped
                for (i, c) in inner.chars().enumerate() {
                    if c == '"' {
                        // There should be a backslash before it
                        if i > 0 {
                            prop_assert_eq!(inner.chars().nth(i-1), Some('\\'));
                        }
                    }
                }
            }

            /// Property: Builder with arbitrary fields produces non-empty output
            #[test]
            fn builder_with_condition_produces_output(
                field in "[a-z]+",
                value in "\\PC*"
            ) {
                let query = JqlBuilder::new().eq(&field, &value).finish();
                prop_assert!(!query.is_empty());
                prop_assert!(query.contains(&field));
            }

            /// Property: Multiple conditions are joined with AND
            #[test]
            fn multiple_conditions_use_and(
                field1 in "[a-z]+",
                value1 in "\\PC{0,20}",
                field2 in "[a-z]+",
                value2 in "\\PC{0,20}"
            ) {
                let query = JqlBuilder::new()
                    .eq(&field1, &value1)
                    .eq(&field2, &value2)
                    .finish();

                if field1 != field2 || value1 != value2 {
                    prop_assert!(query.contains(" AND "));
                }
            }

            /// Property: IN list with arbitrary values doesn't panic
            #[test]
            fn in_list_never_panics(
                field in "[a-z]+",
                values in prop::collection::vec("\\PC{0,20}", 0..10)
            ) {
                let strings: Vec<String> = values.iter().map(|s| s.to_string()).collect();
                let query = JqlBuilder::new().in_list(&field, &strings).finish();

                if !strings.is_empty() {
                    prop_assert!(query.contains(&field));
                    prop_assert!(query.contains(" IN ("));
                }
            }
        }
    }
}
