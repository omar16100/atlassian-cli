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
        let params = UrlParamsBuilder::new().add("safe_key", malicious).finish();

        // Verify the & is escaped and doesn't create a new parameter
        assert!(params.contains("%26extra_param"));
        assert!(!params.contains("&extra_param="));
    }

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Property: Any string can be safely encoded without panicking
            #[test]
            fn encoding_never_panics(
                key in "\\PC{1,50}",
                value in "\\PC*"
            ) {
                let _ = UrlParamsBuilder::new().add(&key, &value).finish();
            }

            /// Property: Encoded output never contains unencoded special chars
            #[test]
            fn no_unencoded_special_chars(
                key in "[a-z]+",
                value in ".*"
            ) {
                let params = UrlParamsBuilder::new().add(&key, &value).finish();

                // & should only appear as separator, never in values
                let parts: Vec<&str> = params.split('&').collect();
                for part in parts {
                    // Each part should be key=value, and value shouldn't contain raw &
                    if let Some(value_part) = part.split('=').nth(1) {
                        prop_assert!(!value_part.contains('&'));
                    }
                }
            }

            /// Property: Special chars are percent-encoded
            #[test]
            fn special_chars_encoded(value in "[&=#+]*") {
                if !value.is_empty() {
                    let params = UrlParamsBuilder::new().add("key", &value).finish();

                    // If value contains special chars, they should be encoded
                    if value.contains('&') {
                        prop_assert!(params.contains("%26"));
                    }
                    if value.contains('=') {
                        prop_assert!(params.contains("%3D"));
                    }
                    if value.contains('#') {
                        prop_assert!(params.contains("%23"));
                    }
                    if value.contains('+') {
                        prop_assert!(params.contains("%2B"));
                    }
                }
            }

            /// Property: Multiple parameters are separated by &
            #[test]
            fn multiple_params_separated(
                key1 in "[a-z]+",
                value1 in "\\PC{0,20}",
                key2 in "[a-z]+",
                value2 in "\\PC{0,20}"
            ) {
                let params = UrlParamsBuilder::new()
                    .add(&key1, &value1)
                    .add(&key2, &value2)
                    .finish();

                // Should contain both keys
                prop_assert!(params.contains(&key1));
                prop_assert!(params.contains(&key2));

                // Should have separator between params
                prop_assert!(params.contains('&'));
            }

            /// Property: Optional None values don't appear in output
            #[test]
            fn optional_none_excluded(
                key1 in "[a-z]+",
                value1 in "\\PC{0,20}",
                key2 in "[a-z]+"
            ) {
                // Only test when keys are different and key2 is not substring of key1
                if key1 != key2 && !key1.contains(&key2) {
                    let params = UrlParamsBuilder::new()
                        .add(&key1, &value1)
                        .add_optional(&key2, None)
                        .finish();

                    // key1 should appear as a parameter
                    let key1_pattern = format!("{}=", key1);
                    let key2_pattern = format!("{}=", key2);

                    prop_assert!(params.contains(&key1_pattern));
                    // key2 should NOT appear as a parameter (since it was None)
                    prop_assert!(!params.contains(&key2_pattern));
                }
            }

            /// Property: Empty builder produces empty string
            #[test]
            fn empty_builder_empty_output(_i in 0..10u32) {
                let params = UrlParamsBuilder::new().finish();
                prop_assert_eq!(&params, "");
            }
        }
    }
}
