//! Encoding for query string components.
//!
//! Path segments go through the guards in the crate root; they reject
//! backslashes, dot segments and encoded separators. Query values are different: a JQL or CQL expression
//! legitimately contains spaces, `=`, `/`, quotes and non-ASCII text, so they
//! are percent-encoded instead of rejected.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

/// Everything except ASCII letters, digits and `-`, `.`, `_`, `~` is encoded.
/// That is the RFC 3986 unreserved set, and the same set the `urlencoding`
/// crate used before this helper replaced it, so existing URLs did not change.
const QUERY_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Percent-encodes `value` for use as a query string key or value.
///
/// Spaces become `%20` (not `+`), so the result is safe in any position of a
/// query string, and non-ASCII text is encoded as UTF-8 bytes.
pub fn encode_query_component(value: &str) -> String {
    utf8_percent_encode(value, QUERY_COMPONENT).to_string()
}

#[cfg(test)]
mod tests {
    use super::encode_query_component as enc;

    #[test]
    fn unreserved_characters_pass_through() {
        assert_eq!(enc("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(enc(""), "");
    }

    #[test]
    fn reserved_and_delimiter_characters_are_encoded() {
        assert_eq!(enc("a b"), "a%20b");
        assert_eq!(enc("+"), "%2B");
        assert_eq!(enc("%"), "%25");
        assert_eq!(enc("/"), "%2F");
        assert_eq!(enc("="), "%3D");
        assert_eq!(enc("&"), "%26");
        assert_eq!(enc("?#"), "%3F%23");
        assert_eq!(enc("\"'"), "%22%27");
    }

    #[test]
    fn jql_round_trips_the_way_the_old_crate_encoded_it() {
        // Expected strings checked against urlencoding 2.1.3 on 16 Oct 2026,
        // the crate this helper replaced.
        assert_eq!(
            enc("project = DEV AND status != Done ORDER BY created DESC"),
            "project%20%3D%20DEV%20AND%20status%20%21%3D%20Done%20ORDER%20BY%20created%20DESC"
        );
        assert_eq!(
            enc("type=page and title~\"Q3 plan\""),
            "type%3Dpage%20and%20title~%22Q3%20plan%22"
        );
    }

    #[test]
    fn non_ascii_is_encoded_as_utf8_bytes() {
        assert_eq!(enc("é"), "%C3%A9");
        assert_eq!(enc("日本"), "%E6%97%A5%E6%9C%AC");
        assert_eq!(enc("naïve café"), "na%C3%AFve%20caf%C3%A9");
    }

    #[test]
    fn already_encoded_input_is_encoded_again() {
        // The helper does not try to detect prior encoding; callers encode once.
        assert_eq!(enc("a%20b"), "a%2520b");
    }
}
