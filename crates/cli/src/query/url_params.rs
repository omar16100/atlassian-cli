use url::form_urlencoded;

/// Builder for URL query parameters with proper escaping
///
/// # Example
/// ```ignore
/// let params = UrlParamsBuilder::new()
///     .add("offset", "0")
///     .add("limit", "50")
///     .add_optional("filter", Some("status=open"))
///     .finish();
///
/// assert_eq!(params, "offset=0&limit=50&filter=status%3Dopen");
/// ```
pub struct UrlParamsBuilder {
    params: Vec<(String, String)>,
}

impl UrlParamsBuilder {
    /// Create a new UrlParamsBuilder
    pub fn new() -> Self {
        Self { params: Vec::new() }
    }

    /// Add a key-value pair to the query parameters
    pub fn add(mut self, key: &str, value: &str) -> Self {
        self.params.push((key.to_string(), value.to_string()));
        self
    }

    /// Add an optional key-value pair (only adds if value is Some)
    pub fn add_optional(mut self, key: &str, value: Option<&str>) -> Self {
        if let Some(v) = value {
            self.params.push((key.to_string(), v.to_string()));
        }
        self
    }

    /// Build the final query string with proper URL encoding
    pub fn finish(self) -> String {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (key, value) in self.params {
            serializer.append_pair(&key, &value);
        }
        serializer.finish()
    }
}

impl Default for UrlParamsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_params_builder_basic() {
        let params = UrlParamsBuilder::new()
            .add("offset", "0")
            .add("limit", "50")
            .finish();

        assert_eq!(params, "offset=0&limit=50");
    }

    #[test]
    fn test_url_params_builder_escaping() {
        let params = UrlParamsBuilder::new()
            .add("filter", "status=open")
            .add("query", "name:\"Test User\"")
            .finish();

        // Verify special characters are URL-encoded
        assert!(params.contains("%3D")); // '=' encoded
        assert!(params.contains("%22")); // '"' encoded
        assert!(params.contains("%3A")); // ':' encoded
    }

    #[test]
    fn test_url_params_builder_optional_some() {
        let params = UrlParamsBuilder::new()
            .add("offset", "0")
            .add_optional("filter", Some("status=open"))
            .finish();

        assert!(params.contains("filter"));
        assert!(params.contains("status"));
    }

    #[test]
    fn test_url_params_builder_optional_none() {
        let params = UrlParamsBuilder::new()
            .add("offset", "0")
            .add_optional("filter", None)
            .finish();

        assert!(!params.contains("filter"));
        assert_eq!(params, "offset=0");
    }

    #[test]
    fn test_url_params_builder_empty() {
        let params = UrlParamsBuilder::new().finish();

        assert_eq!(params, "");
    }

    #[test]
    fn test_url_params_builder_special_chars() {
        let params = UrlParamsBuilder::new()
            .add("key", "value with spaces & special=chars")
            .finish();

        // Verify spaces are encoded as %20 or +
        assert!(params.contains("%20") || params.contains('+'));
        // Verify & is encoded
        assert!(params.contains("%26"));
    }

    #[test]
    fn test_url_params_injection_attempt() {
        // Attempt to inject additional parameters
        let malicious = "value&extra_param=injected";
        let params = UrlParamsBuilder::new()
            .add("safe_key", malicious)
            .finish();

        // Verify the & is escaped and doesn't create a new parameter
        assert!(params.contains("%26extra_param"));
        assert!(!params.contains("&extra_param="));
    }
}
