/// Builder for filter strings with proper escaping
///
/// Used for Bitbucket API filter parameters and similar query DSLs
///
/// # Example
/// ```ignore
/// let filter = FilterBuilder::new()
///     .add_eq("status", "open")
///     .add_gte("created_on", "2024-01-01")
///     .finish();
///
/// // Produces: (status="open" AND created_on>="2024-01-01")
/// ```
pub struct FilterBuilder {
    filters: Vec<String>,
}

impl FilterBuilder {
    /// Create a new FilterBuilder
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    /// Add an equality filter (field="value")
    pub fn add_eq(mut self, field: &str, value: &str) -> Self {
        let escaped = Self::escape_value(value);
        self.filters.push(format!("{}=\"{}\"", field, escaped));
        self
    }

    /// Add a greater-than-or-equal filter (field>="value")
    pub fn add_gte(mut self, field: &str, value: &str) -> Self {
        let escaped = Self::escape_value(value);
        self.filters.push(format!("{}>=\"{}\"", field, escaped));
        self
    }

    /// Add a less-than filter (field<"value")
    pub fn add_lt(mut self, field: &str, value: &str) -> Self {
        let escaped = Self::escape_value(value);
        self.filters.push(format!("{}<\"{}\"", field, escaped));
        self
    }

    /// Add a greater-than filter (field>"value")
    pub fn add_gt(mut self, field: &str, value: &str) -> Self {
        let escaped = Self::escape_value(value);
        self.filters.push(format!("{}>\"{}\"", field, escaped));
        self
    }

    /// Add a less-than-or-equal filter (field<="value")
    pub fn add_lte(mut self, field: &str, value: &str) -> Self {
        let escaped = Self::escape_value(value);
        self.filters.push(format!("{}<=\"{}\"", field, escaped));
        self
    }

    /// Add a not-equal filter (field!="value")
    pub fn add_ne(mut self, field: &str, value: &str) -> Self {
        let escaped = Self::escape_value(value);
        self.filters.push(format!("{}!=\"{}\"", field, escaped));
        self
    }

    /// Build the final filter string
    /// Returns empty string if no filters, single filter without parens, or multiple filters with parens and AND
    pub fn finish(self) -> String {
        match self.filters.len() {
            0 => String::new(),
            1 => self.filters.into_iter().next().unwrap(),
            _ => format!("({})", self.filters.join(" AND ")),
        }
    }

    /// Escape special characters in filter values
    /// Escapes backslashes and double quotes
    fn escape_value(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }
}

impl Default for FilterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_builder_single_eq() {
        let filter = FilterBuilder::new().add_eq("status", "open").finish();

        assert_eq!(filter, "status=\"open\"");
    }

    #[test]
    fn test_filter_builder_multiple_filters() {
        let filter = FilterBuilder::new()
            .add_eq("status", "open")
            .add_gte("created_on", "2024-01-01")
            .finish();

        assert_eq!(filter, "(status=\"open\" AND created_on>=\"2024-01-01\")");
    }

    #[test]
    fn test_filter_builder_all_operators() {
        let filter = FilterBuilder::new()
            .add_eq("field1", "value1")
            .add_ne("field2", "value2")
            .add_gt("field3", "value3")
            .add_gte("field4", "value4")
            .add_lt("field5", "value5")
            .add_lte("field6", "value6")
            .finish();

        assert!(filter.contains("field1=\"value1\""));
        assert!(filter.contains("field2!=\"value2\""));
        assert!(filter.contains("field3>\"value3\""));
        assert!(filter.contains("field4>=\"value4\""));
        assert!(filter.contains("field5<\"value5\""));
        assert!(filter.contains("field6<=\"value6\""));
        assert!(filter.starts_with('('));
        assert!(filter.ends_with(')'));
    }

    #[test]
    fn test_filter_builder_empty() {
        let filter = FilterBuilder::new().finish();

        assert_eq!(filter, "");
    }

    #[test]
    fn test_filter_builder_quote_escaping() {
        let filter = FilterBuilder::new()
            .add_eq("field", "value with \"quotes\"")
            .finish();

        // Verify quotes are escaped with backslash
        assert!(filter.contains("\\\"quotes\\\""));
        assert!(!filter.contains("\"quotes\""));
    }

    #[test]
    fn test_filter_builder_backslash_escaping() {
        let filter = FilterBuilder::new()
            .add_eq("field", "path\\with\\backslashes")
            .finish();

        // Verify backslashes are escaped
        assert!(filter.contains("\\\\"));
    }

    #[test]
    fn test_filter_injection_attempt() {
        // Attempt to inject additional filter clauses
        let malicious = "value\" OR admin=\"true";
        let filter = FilterBuilder::new().add_eq("user", malicious).finish();

        // The entire malicious string should be treated as a single value
        // with all quotes escaped
        assert_eq!(filter, "user=\"value\\\" OR admin=\\\"true\"");

        // Verify quotes are properly escaped with backslashes
        assert!(filter.contains("\\\""), "Quotes should be escaped");

        // The filter should start with field= and have outer quotes
        assert!(filter.starts_with("user=\""));
        assert!(filter.ends_with("\""));

        // Count the backslash-escaped quotes - should have 2 (one for each injected quote)
        let escaped_quotes = filter.matches("\\\"").count();
        assert_eq!(
            escaped_quotes, 2,
            "Should have 2 escaped quotes from the malicious input"
        );
    }

    #[test]
    fn test_filter_builder_gte() {
        let filter = FilterBuilder::new()
            .add_gte("created_on", "2024-01-01T00:00:00")
            .finish();

        assert_eq!(filter, "created_on>=\"2024-01-01T00:00:00\"");
    }

    #[test]
    fn test_filter_builder_lt() {
        let filter = FilterBuilder::new()
            .add_lt("updated_at", "2024-12-31T23:59:59")
            .finish();

        assert_eq!(filter, "updated_at<\"2024-12-31T23:59:59\"");
    }
}
