use anyhow::Context;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use url::Url;

pub struct BitbucketContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
    /// true if using Bearer auth (access tokens), false for Basic auth
    pub is_bearer: bool,
}

impl BitbucketContext<'_> {
    pub async fn verify_auth(&self) -> anyhow::Result<()> {
        // Bearer tokens (access tokens) can't use /2.0/user — use /2.0/workspaces
        let endpoint = if self.is_bearer {
            "/2.0/workspaces"
        } else {
            "/2.0/user"
        };
        let _: serde_json::Value = self.client.get(endpoint).await.context(
            "Failed to verify Bitbucket access. Run: atlassian-cli auth test --bitbucket",
        )?;
        Ok(())
    }
}

/// Extract Bitbucket workspace from a URL.
/// Supports:
/// - https://bitbucket.org/{workspace}
/// - https://bitbucket.org/{workspace}/...
pub fn extract_workspace_from_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if parsed.host_str() == Some("bitbucket.org") {
        let path = parsed.path().trim_start_matches('/');
        let workspace = path.split('/').next()?;
        if !workspace.is_empty() {
            return Some(workspace.to_string());
        }
    }
    None
}

/// Percent-encode a ref name for interpolation into a URL path, rejecting names
/// that would change which resource the path addresses.
///
/// Two distinct hazards, and encoding alone only covers the first.
///
/// **Characters that restructure the URL.** Git permits `#` in a branch name,
/// and `#` starts a fragment. Interpolated raw, `main#old` resolves to
/// `.../refs/branches/main` with `old` as a fragment that is never transmitted,
/// so a `DELETE` aimed at `main#old` lands on `main` — through the
/// protected-name check, while the prompt still says `main#old`. Everything
/// outside the RFC 3986 unreserved set is encoded, which also covers the `%`
/// that would let `foo%23` be re-decoded server-side into a different ref.
///
/// **Dot segments.** `/` must be preserved, because `feature/login` is an
/// ordinary branch name whose slashes are real separators. But preserving `/`
/// and `.` together lets `Url::join` normalise the path away:
/// `feature/../main` resolves to `.../refs/branches/main`, and
/// `a/../../../../../../repositories/w2/r2` turns a branch delete into
/// `DELETE /2.0/repositories/w2/r2` — a whole repository, same-origin, so
/// `safe_join` passes it. Encoding cannot fix this without also encoding
/// legitimate separators, so such names are **rejected** instead.
///
/// Rejecting loses nothing: `git check-ref-format` already forbids `.` and `..`
/// path components, empty components and a trailing `/`, so no real ref can
/// take this path. The guard exists for names typed on the command line and for
/// values read back out of an API response.
pub fn encode_ref_path(name: &str) -> anyhow::Result<String> {
    if name.is_empty() {
        anyhow::bail!("Ref name cannot be empty");
    }
    if name.starts_with('/') || name.ends_with('/') {
        anyhow::bail!("Invalid ref name {name:?}: must not begin or end with '/'");
    }
    for segment in name.split('/') {
        if segment.is_empty() {
            anyhow::bail!("Invalid ref name {name:?}: empty path component");
        }
        if segment == "." || segment == ".." {
            anyhow::bail!(
                "Invalid ref name {name:?}: '{segment}' component would change which \
                 resource the request addresses"
            );
        }
    }

    let mut out = String::with_capacity(name.len());
    for byte in name.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    Ok(out)
}

/// Page size for a list command's first request.
///
/// `--limit 0` means "everything", so it asks for a full page rather than
/// clamping to zero and fetching nothing.
pub fn page_size(limit: usize) -> usize {
    if limit == 0 {
        100
    } else {
        limit.clamp(1, 100)
    }
}

/// Warn on stderr when a list came back incomplete.
///
/// stderr, so a piped `-f json` result stays machine-readable. The envelope
/// carries this structurally for the commands wired to `ListMeta`; this is the
/// signal for everything else, and for the tabular formats, which have no field
/// to put it in.
pub fn warn_if_truncated(page: &atlassian_cli_api::pagination::PageInfo, shown: usize, noun: &str) {
    warn_if_truncated_with(page, shown, noun, true)
}

/// As [`warn_if_truncated`], for commands that have no `--limit` flag.
///
/// `bb webhook list` and `bb ssh-key list` take no limit, so advising the user
/// to raise one names a flag that does not exist. There the shortfall can only
/// come from the request budget, and the honest thing is to say the result is
/// incomplete without prescribing a remedy that is unavailable.
pub fn warn_if_truncated_with(
    page: &atlassian_cli_api::pagination::PageInfo,
    shown: usize,
    noun: &str,
    has_limit_flag: bool,
) {
    if !page.truncated {
        return;
    }
    if has_limit_flag {
        eprintln!(
            "warning: showing {shown} {noun}; more exist. Raise --limit, or use --limit 0 for all."
        );
    } else {
        eprintln!("warning: showing {shown} {noun}; the list is incomplete.");
    }
}

/// Accept only an identifier that cannot restructure a URL, without changing
/// how it is sent.
///
/// This started as a blacklist of `/`, `#`, `%` and dot components, derived
/// from the characters a review had named. That was the wrong method and it
/// leaked: the WHATWG parser behind `Url::join` treats `\\` as `/`, so `..\\`
/// still resolved to the parent — the repository endpoint, reached by a delete
/// with no confirmation. It also strips tab, CR and LF before parsing, so
/// `".\t."` became `..`, and it splits on a raw `?`. Enumerating what the
/// parser does is a losing game; this permits only what is known safe.
///
/// Used where the value is also reused verbatim outside the URL — a pipeline
/// uuid is trimmed of its braces to build a browser link — so it cannot simply
/// be percent-encoded in place. Everywhere else, prefer
/// [`encode_path_segment`], whose output is inert by construction.
pub fn accept_safe_identifier(value: &str, what: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        anyhow::bail!("{what} cannot be empty");
    }
    let permitted = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '{' | '}');
    if let Some(bad) = value.chars().find(|c| !permitted(*c)) {
        anyhow::bail!(
            "Invalid {what} {value:?}: {bad:?} is not permitted in an identifier, because it can \
             change which resource the request addresses"
        );
    }
    if value.chars().all(|c| c == '.') {
        anyhow::bail!("Invalid {what} {value:?}: a dot component would retarget the request");
    }
    Ok(())
}

/// Percent-encode a single path segment, rejecting anything that would make it
/// more than one.
///
/// `encode_ref_path` deliberately preserves `/`, because `feature/login` is one
/// branch name spanning two path segments. A repository slug, a webhook uuid or
/// a key id is never like that: Bitbucket slugs are `[A-Za-z0-9._-]`. Reusing
/// the ref helper for them left a hole -- `bb repo delete "myrepo/refs/branches/main"`
/// passed the guard, reached the branch-delete endpoint, deleted a branch, and
/// reported "Repository myrepo/refs/branches/main deleted".
///
/// Percent-encoding a brace-wrapped uuid is **not** a change to what Bitbucket
/// receives, contrary to what an earlier revision of this file asserted.
/// `Url::join` already encodes `{` and `}`: a raw `{abc-123}` and a
/// pre-encoded `%7Babc-123%7D` produce byte-identical request paths. Verified
/// against the `url` version in the lockfile. That mistaken belief is why the
/// weaker guard above exists at all, and why it is now confined to the one
/// case that genuinely cannot encode.
pub fn encode_path_segment(value: &str) -> anyhow::Result<String> {
    if value.contains('/') {
        anyhow::bail!(
            "Invalid identifier {value:?}: '/' would address a different resource than the one named"
        );
    }
    encode_ref_path(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_whitelist_rejects_what_the_url_parser_reinterprets() {
        assert!(accept_safe_identifier("{abc-123}", "webhook uuid").is_ok());
        assert!(accept_safe_identifier("d6a3f1", "key id").is_ok());
        assert!(accept_safe_identifier("build.42", "pipeline id").is_ok());

        // Each of these was demonstrated to retarget the request through
        // Url::join: backslash is a separator, tab/CR/LF are stripped before
        // parsing, `?` starts a query, and a bare space trims to nothing.
        for bad in [
            "..", ".", "...", "a/b", "abc#x", "abc%2e", "..\\", "a\\..\\x", ".\t.", ".\n.",
            "{u}?x=1", " ", "",
        ] {
            assert!(
                accept_safe_identifier(bad, "webhook uuid").is_err(),
                "{bad:?} must be rejected"
            );
        }
    }

    /// The claim the weaker guard was originally justified by, pinned so it
    /// cannot be reasserted: encoding a braced uuid changes nothing on the wire.
    #[test]
    fn encoding_a_braced_uuid_does_not_change_the_request_path() {
        let base = Url::parse("https://api.bitbucket.org/").unwrap();
        let raw = base.join("2.0/repositories/w/r/hooks/{abc-123}").unwrap();
        let encoded = base
            .join(&format!(
                "2.0/repositories/w/r/hooks/{}",
                encode_path_segment("{abc-123}").unwrap()
            ))
            .unwrap();
        assert_eq!(raw.path(), encoded.path());
        assert_eq!(raw.path(), "/2.0/repositories/w/r/hooks/%7Babc-123%7D");
    }

    /// The hole this exists to close: a slug with a `/` reached the
    /// branch-delete endpoint while the message still said "repository".
    #[test]
    fn a_slug_containing_a_slash_is_rejected() {
        assert!(encode_path_segment("myrepo/refs/branches/main").is_err());
        assert!(encode_path_segment("a/b").is_err());
    }

    #[test]
    fn an_ordinary_slug_passes_and_is_still_encoded() {
        assert_eq!(encode_path_segment("my-repo.v2").unwrap(), "my-repo.v2");
        assert_eq!(encode_path_segment("odd#name").unwrap(), "odd%23name");
        assert!(encode_path_segment("..").is_err());
        assert!(encode_path_segment("").is_err());
    }

    #[test]
    fn a_zero_limit_asks_for_a_full_page() {
        // Clamping to 0 here would fetch nothing while claiming to fetch all.
        assert_eq!(page_size(0), 100);
        assert_eq!(page_size(25), 25);
        assert_eq!(page_size(500), 100);
        assert_eq!(page_size(1), 1);
    }

    /// The bug this exists for: `#` truncates the path at the client, so the
    /// request lands on a different — and possibly protected — ref.
    #[test]
    fn hash_in_a_branch_name_is_encoded() {
        assert_eq!(encode_ref_path("main#old").unwrap(), "main%23old");
    }

    #[test]
    fn slashes_are_preserved_as_path_separators() {
        assert_eq!(encode_ref_path("feature/login").unwrap(), "feature/login");
        assert_eq!(
            encode_ref_path("release/2026.09").unwrap(),
            "release/2026.09"
        );
    }

    #[test]
    fn percent_is_encoded_so_the_server_cannot_re_decode_it() {
        assert_eq!(encode_ref_path("foo%23").unwrap(), "foo%2523");
    }

    #[test]
    fn ordinary_names_are_unchanged() {
        for name in ["main", "develop", "feature/a-b_c.d~e"] {
            assert_eq!(encode_ref_path(name).unwrap(), name);
        }
    }

    #[test]
    fn non_ascii_is_utf8_percent_encoded() {
        assert_eq!(encode_ref_path("função").unwrap(), "fun%C3%A7%C3%A3o");
    }

    /// Guards the actual consequence, not just the string transform: after
    /// encoding, resolving the path must still target the original ref.
    #[test]
    fn encoded_path_resolves_to_the_intended_ref() {
        let base = Url::parse("https://api.bitbucket.org/").unwrap();
        let raw = base
            .join("2.0/repositories/w/r/refs/branches/main#old")
            .unwrap();
        assert_eq!(raw.path(), "/2.0/repositories/w/r/refs/branches/main");

        let encoded = base
            .join(&format!(
                "2.0/repositories/w/r/refs/branches/{}",
                encode_ref_path("main#old").unwrap()
            ))
            .unwrap();
        assert_eq!(
            encoded.path(),
            "/2.0/repositories/w/r/refs/branches/main%23old"
        );
        assert_eq!(encoded.fragment(), None);
    }

    /// The hole the first version of this function left open. `Url::join`
    /// normalises dot segments, so a preserved `/` plus a preserved `.` let a
    /// branch name retarget the request at another resource entirely.
    #[test]
    fn dot_segments_are_rejected() {
        for name in [
            "feature/../main",
            "a/../../../../../../repositories/w2/r2",
            "./main",
            "main/.",
            "a/./b",
        ] {
            assert!(
                encode_ref_path(name).is_err(),
                "{name} must be rejected, not encoded"
            );
        }
    }

    #[test]
    fn empty_and_edge_slash_names_are_rejected() {
        for name in ["", "/main", "main/", "a//b"] {
            assert!(encode_ref_path(name).is_err(), "{name} must be rejected");
        }
    }

    /// A dot is fine inside a component; only whole-component dots retarget.
    #[test]
    fn dots_within_a_component_are_allowed() {
        assert_eq!(
            encode_ref_path("release/2026.09.1").unwrap(),
            "release/2026.09.1"
        );
        assert_eq!(encode_ref_path("v1.2.3").unwrap(), "v1.2.3");
        assert_eq!(encode_ref_path("..hidden").unwrap(), "..hidden");
    }

    /// Guards the consequence: a rejected name never reaches a URL at all, and
    /// an accepted one still resolves to itself.
    #[test]
    fn no_accepted_name_can_retarget_the_path() {
        let base = Url::parse("https://api.bitbucket.org/").unwrap();
        for name in ["main", "feature/login", "main#old", "v1.2.3"] {
            let encoded = encode_ref_path(name).unwrap();
            let url = base
                .join(&format!("2.0/repositories/w/r/refs/branches/{encoded}"))
                .unwrap();
            assert!(
                url.path()
                    .starts_with("/2.0/repositories/w/r/refs/branches/"),
                "{name} escaped its prefix: {}",
                url.path()
            );
            assert_eq!(url.fragment(), None, "{name} produced a fragment");
        }
    }

    #[test]
    fn test_extract_workspace_from_bitbucket_url() {
        assert_eq!(
            extract_workspace_from_url("https://bitbucket.org/myworkspace"),
            Some("myworkspace".to_string())
        );
    }

    #[test]
    fn test_extract_workspace_from_bitbucket_url_with_path() {
        assert_eq!(
            extract_workspace_from_url("https://bitbucket.org/myworkspace/some/repo"),
            Some("myworkspace".to_string())
        );
    }

    #[test]
    fn test_extract_workspace_from_non_bitbucket_url() {
        assert_eq!(
            extract_workspace_from_url("https://example.atlassian.net"),
            None
        );
    }

    #[test]
    fn test_extract_workspace_from_root_url() {
        assert_eq!(extract_workspace_from_url("https://bitbucket.org/"), None);
    }

    #[test]
    fn test_extract_workspace_from_empty_path() {
        assert_eq!(extract_workspace_from_url("https://bitbucket.org"), None);
    }
}
