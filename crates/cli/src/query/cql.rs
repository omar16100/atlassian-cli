/// Builder for constructing CQL (Confluence Query Language) queries from filter parameters
pub struct CqlBuilder {
    conditions: Vec<String>,
}

impl CqlBuilder {
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

    /// Build the final CQL query string
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
            ("creator" | "contributor" | "mention", "@me") => "currentUser()".to_string(),
            // Default: escape and quote
            _ => Self::escape_and_quote(value),
        }
    }
}

impl Default for CqlBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_builder() {
        let query = CqlBuilder::new().finish();
        assert_eq!(query, "");
    }

    #[test]
    fn test_single_eq() {
        let query = CqlBuilder::new().eq("space", "ENG").finish();
        assert_eq!(query, "space = \"ENG\"");
    }

    #[test]
    fn test_creator_me_shorthand() {
        let query = CqlBuilder::new().eq("creator", "@me").finish();
        assert_eq!(query, "creator = currentUser()");
    }

    #[test]
    fn test_multiple_conditions() {
        let query = CqlBuilder::new()
            .eq("space", "ENG")
            .eq("type", "page")
            .finish();
        assert_eq!(query, "space = \"ENG\" AND type = \"page\"");
    }

    #[test]
    fn test_in_list_single() {
        let query = CqlBuilder::new()
            .in_list("type", &[String::from("page")])
            .finish();
        assert_eq!(query, "type IN (\"page\")");
    }

    #[test]
    fn test_in_list_multiple() {
        let query = CqlBuilder::new()
            .in_list("type", &[String::from("page"), String::from("blogpost")])
            .finish();
        assert_eq!(query, "type IN (\"page\", \"blogpost\")");
    }

    #[test]
    fn test_in_list_empty() {
        let query = CqlBuilder::new().in_list("type", &[]).finish();
        assert_eq!(query, "");
    }

    #[test]
    fn test_contains_title() {
        let query = CqlBuilder::new().contains("title", "API").finish();
        assert_eq!(query, "title ~ \"API\"");
    }

    #[test]
    fn test_contains_text() {
        let query = CqlBuilder::new().contains("text", "documentation").finish();
        assert_eq!(query, "text ~ \"documentation\"");
    }

    #[test]
    fn test_quote_escape() {
        let query = CqlBuilder::new()
            .eq("title", "Page \"with quotes\"")
            .finish();
        assert_eq!(query, "title = \"Page \\\"with quotes\\\"\"");
    }

    #[test]
    fn test_backslash_escape() {
        let query = CqlBuilder::new().eq("title", "Path\\to\\page").finish();
        assert_eq!(query, "title = \"Path\\\\to\\\\page\"");
    }

    #[test]
    fn test_complex_query() {
        let query = CqlBuilder::new()
            .eq("space", "ENG")
            .eq("type", "page")
            .eq("creator", "@me")
            .in_list("label", &[String::from("api"), String::from("backend")])
            .contains("title", "REST")
            .finish();

        assert_eq!(
            query,
            "space = \"ENG\" AND type = \"page\" AND creator = currentUser() AND label IN (\"api\", \"backend\") AND title ~ \"REST\""
        );
    }
}
