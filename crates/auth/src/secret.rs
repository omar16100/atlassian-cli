use secrecy::{ExposeSecret, Secret};
use std::fmt;

/// A wrapper around a secret token that prevents accidental exposure.
/// Uses `secrecy::Secret` to ensure tokens are never logged or displayed.
/// Automatically zeroed on drop to prevent memory disclosure.
pub struct SecretToken(Secret<String>);

impl SecretToken {
    /// Create a new SecretToken from a String.
    pub fn new(token: String) -> Self {
        Self(Secret::new(token))
    }

    /// Expose the secret token for use in authentication.
    /// This should only be called when actually using the token (e.g., in HTTP headers).
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretToken([REDACTED])")
    }
}

impl Drop for SecretToken {
    fn drop(&mut self) {
        // Zeroizing is handled by secrecy::Secret internally
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_token_creation() {
        let token = SecretToken::new("my-secret-token".to_string());
        assert_eq!(token.expose(), "my-secret-token");
    }

    #[test]
    fn test_secret_token_debug_redacted() {
        let token = SecretToken::new("my-secret-token".to_string());
        let debug_output = format!("{:?}", token);
        assert_eq!(debug_output, "SecretToken([REDACTED])");
        assert!(!debug_output.contains("my-secret-token"));
    }

    #[test]
    fn test_secret_token_no_clone() {
        // This test documents that SecretToken cannot be cloned,
        // preventing accidental duplication of secrets in memory.
        // Uncommenting the following line should cause a compilation error:
        // let token = SecretToken::new("test".to_string());
        // let _cloned = token.clone();
    }
}
