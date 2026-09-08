//! Following a paginated API to completion.
//!
//! The list commands each requested one page and rendered whatever came back.
//! Bitbucket defaults to 10 items on some collections and 20 on others, so a
//! pipeline with 11 steps reported 10, and nothing in the output distinguished
//! that from a complete answer. Jira's `/search/jql` caps `maxResults` at 100
//! server-side regardless of what is asked for, so a query returning exactly
//! 100 was indistinguishable from a complete result. Both are worse than an
//! error, because the output looks authoritative.
//!
//! The behaviour this replaces was reimplemented three times by hand inside
//! `bitbucket/pipelines.rs` and `bitbucket/variables.rs`, in three different
//! forms with three different levels of care about the URL the server handed
//! back. It belongs here, next to `safe_join`, which is the code that already
//! knows what a trusted origin is.
//!
//! # Why the wrapper is generic and the driver is not
//!
//! Every list endpoint wraps its items under a different key: Bitbucket uses
//! `values` uniformly, Jira's search uses `issues`. A single
//! `fetch_paged<T>(path) -> Vec<T>` cannot work, because `ApiClient::get::<T>`
//! deserializes the whole body and nothing tells it which key to look under.
//! Passing the key as a string would mean deserializing to `serde_json::Value`
//! and re-parsing, which costs a second parse and throws away the type errors
//! that make the response structs worth having.
//!
//! So the *wrapper* is generic instead: `BitbucketPage<T>` and `JiraPage<T>`
//! both implement [`Page`], the call sites name the wrapper, and their own
//! bespoke `StepList` / `SearchResponse` structs go away. Two impls in total,
//! not one per call site.

use serde::de::DeserializeOwned;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::error::Result;
use crate::ApiClient;

/// How to ask for the next page.
///
/// The two products disagree on the mechanism, and the difference is not
/// cosmetic. Bitbucket hands back an absolute URL that already carries the
/// cursor; Jira hands back an opaque token that the caller must place in a
/// query parameter itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Continuation {
    /// An absolute URL from the server. Passed through `safe_join`, so a
    /// response pointing at another origin is rejected rather than followed.
    Url(String),
    /// An opaque token, plus the query parameter it belongs in. The parameter
    /// name travels with the token so that this crate never has to hardcode a
    /// detail of one Jira endpoint.
    Token { param: String, value: String },
}

/// What a page of results tells us beyond the items themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageInfo {
    /// The server's own count of matching items, where it reports one.
    ///
    /// Frequently `None`. Jira's `/search/jql` does not return a total at all,
    /// and Bitbucket omits `size` on collections it considers expensive. A
    /// caller must not present the absence of a total as zero.
    pub total: Option<u64>,
    /// Whether results were cut short, by the caller's limit or by the request
    /// budget. When true, the answer is incomplete and must be labelled so.
    pub truncated: bool,
    /// Where the next page would have started, when `truncated`.
    pub next: Option<Continuation>,
}

/// One page of a paginated response.
pub trait Page: DeserializeOwned {
    type Item;

    /// Consume the page into its items and its cursor.
    ///
    /// Takes `self` because each page is freshly deserialized and never needed
    /// afterwards; this avoids cloning every item out of it.
    fn into_parts(self) -> (Vec<Self::Item>, Option<Continuation>, Option<u64>);
}

/// A Bitbucket collection page.
#[derive(Debug, Clone, Deserialize)]
pub struct BitbucketPage<T> {
    /// Required, deliberately. The hand-rolled wrappers this replaces all
    /// demanded the key, so a 200 whose body lacks it was a loud parse error.
    /// Defaulting it to empty would turn a malformed response into an
    /// authoritative-looking "nothing here" -- and `pipeline_has_failed_steps`
    /// would read that as success.
    pub values: Vec<T>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

impl<T: DeserializeOwned> Page for BitbucketPage<T> {
    type Item = T;

    fn into_parts(self) -> (Vec<T>, Option<Continuation>, Option<u64>) {
        let cursor = self
            .next
            .filter(|url| !url.trim().is_empty())
            .map(Continuation::Url);
        (self.values, cursor, self.size)
    }
}

/// A Jira `/search/jql` page.
#[derive(Debug, Clone, Deserialize)]
pub struct JiraPage<T> {
    /// Required, for the same reason as `BitbucketPage::values`.
    pub issues: Vec<T>,
    #[serde(default, rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    #[serde(default, rename = "isLast")]
    pub is_last: Option<bool>,
}

/// The query parameter Jira's token-based search expects.
pub const JIRA_PAGE_TOKEN_PARAM: &str = "nextPageToken";

impl<T: DeserializeOwned> Page for JiraPage<T> {
    type Item = T;

    fn into_parts(self) -> (Vec<T>, Option<Continuation>, Option<u64>) {
        // `isLast` is authoritative when present. A token can still be echoed
        // on the final page, and following it produces an endless loop of empty
        // results.
        let finished = self.is_last.unwrap_or(false);
        let cursor = if finished {
            None
        } else {
            self.next_page_token
                .filter(|token| !token.trim().is_empty())
                .map(|value| Continuation::Token {
                    param: JIRA_PAGE_TOKEN_PARAM.to_string(),
                    value,
                })
        };
        // This endpoint reports no total; saying so explicitly is the point.
        (self.issues, cursor, None)
    }
}

/// A Jira page from an endpoint that paginates by offset.
///
/// Jira has two pagination styles and they are not interchangeable.
/// `/search/jql` uses an opaque `nextPageToken` ([`JiraPage`]); the classic
/// endpoints -- project search, webhooks, field and workflow lists -- return
/// `startAt`/`maxResults`/`total`/`isLast` and expect the caller to advance an
/// offset itself.
///
/// The module this replaced modelled only this second shape, and modelled it
/// for an endpoint that had since moved to the first, which is why nothing
/// adopted it. Both are supported now, each where it applies.
#[derive(Debug, Clone, Deserialize)]
pub struct JiraOffsetPage<T> {
    pub values: Vec<T>,
    #[serde(default, rename = "startAt")]
    pub start_at: Option<u64>,
    #[serde(default, rename = "maxResults")]
    pub max_results: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default, rename = "isLast")]
    pub is_last: Option<bool>,
}

/// The query parameter Jira's offset-paged endpoints advance.
pub const JIRA_START_AT_PARAM: &str = "startAt";

impl<T: DeserializeOwned> Page for JiraOffsetPage<T> {
    type Item = T;

    fn into_parts(self) -> (Vec<T>, Option<Continuation>, Option<u64>) {
        let start = self.start_at.unwrap_or(0);
        let returned = self.values.len() as u64;
        let next_offset = start + returned;

        // `isLast` is authoritative where the endpoint sends it. Otherwise fall
        // back to the arithmetic, and treat an empty page as the end so a
        // server that omits both cannot produce an endless walk.
        let finished = match self.is_last {
            Some(is_last) => is_last,
            None => match self.total {
                Some(total) => next_offset >= total,
                None => returned == 0,
            },
        };

        let cursor = if finished || returned == 0 {
            None
        } else {
            Some(Continuation::Token {
                param: JIRA_START_AT_PARAM.to_string(),
                value: next_offset.to_string(),
            })
        };

        (self.values, cursor, self.total)
    }
}

/// How many items to collect, and how hard to work for them.
#[derive(Debug, Clone, Copy)]
pub struct PageLimits {
    /// Stop once this many items are held. `None` means everything.
    pub limit: Option<usize>,
    /// Maximum number of HTTP requests, counting the first.
    ///
    /// Separate from `limit`, and not a substitute for it: `limit` is what the
    /// user asked for, this is the guard against a mistyped query walking a
    /// large instance. Exhausting it marks the result truncated rather than
    /// failing, so a partial answer is still labelled honestly.
    pub budget: usize,
}

impl PageLimits {
    pub const DEFAULT_BUDGET: usize = 50;

    pub fn new(limit: Option<usize>) -> Self {
        Self {
            limit,
            budget: Self::DEFAULT_BUDGET,
        }
    }

    /// Build limits from a CLI `--limit`, where **0 means everything**.
    ///
    /// The commands advertise `--limit 0` for "all". Passing it straight
    /// through as `Some(0)` made `items.len() >= 0` true on the first page, so
    /// the result was truncated to nothing and the warning cheerfully advised
    /// using the flag that had just emptied it.
    pub fn from_cli_limit(limit: usize) -> Self {
        Self::new(if limit == 0 { None } else { Some(limit) })
    }

    pub fn with_budget(mut self, budget: usize) -> Self {
        self.budget = budget;
        self
    }
}

/// Follow a paginated endpoint, collecting items until the limit, the budget,
/// or the data runs out.
///
/// `path` is the first request, and it should carry whatever page-size
/// parameter the endpoint wants. Subsequent requests come from the server's own
/// cursor, so the caller never rebuilds the query itself — appending a token to
/// a path that already has one is how a third page ends up with two
/// `nextPageToken` parameters.
pub async fn fetch_paged<P: Page>(
    client: &ApiClient,
    path: &str,
    limits: PageLimits,
) -> Result<(Vec<P::Item>, PageInfo)> {
    let mut items: Vec<P::Item> = Vec::new();
    let mut info = PageInfo::default();
    let mut request = path.to_string();

    for attempt in 0..limits.budget.max(1) {
        debug!(request = %request, attempt, "Fetching page");

        let page: P = client.get(&request).await?;
        let (page_items, cursor, total) = page.into_parts();

        if total.is_some() {
            info.total = total;
        }

        let empty_page = page_items.is_empty();
        items.extend(page_items);

        // The caller's limit wins over anything the server would still offer.
        if let Some(limit) = limits.limit {
            if items.len() >= limit {
                // Compare before truncating. The previous form asked whether
                // `items.len() < total`, but Jira reports no total at all, so a
                // final page that overshot the limit was reported complete
                // while silently dropping rows -- the exact failure this module
                // exists to remove.
                let dropped = items.len() > limit;
                items.truncate(limit);
                info.truncated = dropped || cursor.is_some();
                info.next = cursor;
                return Ok((items, info));
            }
        }

        let Some(cursor) = cursor else {
            // No cursor: this was the last page, and the result is complete.
            return Ok((items, info));
        };

        // A server that keeps handing back a cursor with no items would
        // otherwise spin until the budget runs out.
        if empty_page {
            // The server still claims more exists, so this is incomplete, not
            // complete. Saying otherwise would let bulk.rs's `truncated` bail
            // pass and confirm a deletion against a partial list.
            warn!("Stopping pagination: the server returned an empty page with a cursor");
            info.truncated = true;
            info.next = Some(cursor);
            return Ok((items, info));
        }

        if attempt + 1 >= limits.budget.max(1) {
            warn!(
                budget = limits.budget,
                collected = items.len(),
                "Stopping pagination: request budget exhausted; the result is incomplete"
            );
            info.truncated = true;
            info.next = Some(cursor);
            return Ok((items, info));
        }

        request = match cursor {
            // `safe_join` at the client rejects a foreign origin, so a
            // server-supplied URL cannot redirect us off-site.
            Continuation::Url(url) => url,
            Continuation::Token { param, value } => append_query(path, &param, &value),
        };
    }

    Ok((items, info))
}

/// Add or replace a query parameter on a path.
///
/// Replacing matters: the token goes on the *original* path each time, and
/// naively appending would leave the previous page's token in place, so page
/// three would carry two of them.
fn append_query(path: &str, key: &str, value: &str) -> String {
    let (base, query) = match path.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (path, None),
    };

    let mut pairs: Vec<String> = query
        .map(|q| {
            q.split('&')
                .filter(|pair| !pair.is_empty())
                .filter(|pair| {
                    let name = pair.split('=').next().unwrap_or("");
                    name != key
                })
                .map(|pair| pair.to_string())
                .collect()
        })
        .unwrap_or_default();

    pairs.push(format!(
        "{}={}",
        encode_query_component(key),
        encode_query_component(value)
    ));

    format!("{base}?{}", pairs.join("&"))
}

/// Percent-encode a query key or value.
///
/// Written out rather than pulled from a crate because `crates/api` has no
/// encoding dependency, and because `form_urlencoded` renders a space as `+`,
/// which is correct for form bodies and merely usually-accepted in a query
/// string. Everything outside the RFC 3986 unreserved set is escaped.
fn encode_query_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Item {
        name: String,
    }

    fn bb_page(value: serde_json::Value) -> BitbucketPage<Item> {
        serde_json::from_value(value).unwrap()
    }

    fn jira_page(value: serde_json::Value) -> JiraPage<Item> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_bitbucket_page_yields_its_next_url() {
        let page = bb_page(json!({
            "values": [{"name": "a"}],
            "next": "https://api.bitbucket.org/2.0/x?page=2",
            "size": 7
        }));
        let (items, cursor, total) = page.into_parts();
        assert_eq!(items.len(), 1);
        assert_eq!(
            cursor,
            Some(Continuation::Url(
                "https://api.bitbucket.org/2.0/x?page=2".to_string()
            ))
        );
        assert_eq!(total, Some(7));
    }

    #[test]
    fn a_bitbucket_page_without_next_is_the_last() {
        let page = bb_page(json!({"values": [{"name": "a"}]}));
        assert_eq!(page.into_parts().1, None);
    }

    /// An empty `next` is not a cursor. Following it would re-request the
    /// collection root forever.
    #[test]
    fn a_blank_next_is_not_a_cursor() {
        let page = bb_page(json!({"values": [], "next": "   "}));
        assert_eq!(page.into_parts().1, None);
    }

    /// `isLast` beats a token. Jira can echo a token on the final page, and
    /// following it returns empty results indefinitely.
    #[test]
    fn is_last_overrides_a_trailing_jira_token() {
        let page = jira_page(json!({
            "issues": [{"name": "a"}],
            "nextPageToken": "abc",
            "isLast": true
        }));
        assert_eq!(page.into_parts().1, None);
    }

    #[test]
    fn a_jira_token_carries_its_parameter_name() {
        let page = jira_page(json!({"issues": [], "nextPageToken": "abc"}));
        assert_eq!(
            page.into_parts().1,
            Some(Continuation::Token {
                param: JIRA_PAGE_TOKEN_PARAM.to_string(),
                value: "abc".to_string()
            })
        );
    }

    /// The endpoint reports no total, and pretending otherwise would let a
    /// caller render a confident, wrong count.
    #[test]
    fn jira_reports_no_total() {
        let page = jira_page(json!({"issues": [{"name": "a"}]}));
        assert_eq!(page.into_parts().2, None);
    }

    // ---- driver tests, over a mock server ----

    use wiremock::matchers::{method, path as path_matcher, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_for(server: &MockServer) -> ApiClient {
        ApiClient::new(server.uri()).unwrap()
    }

    /// The reported bug, in miniature: a collection whose first page is not the
    /// whole answer must not be reported as if it were.
    #[tokio::test]
    async fn a_bitbucket_collection_is_followed_to_the_end() {
        let server = MockServer::start().await;
        let page_two = format!("{}/items?page=2", server.uri());

        Mock::given(method("GET"))
            .and(path_matcher("/items"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [{"name": "c"}]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_matcher("/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [{"name": "a"}, {"name": "b"}],
                "next": page_two,
                "size": 3
            })))
            .mount(&server)
            .await;

        let (items, info) = fetch_paged::<BitbucketPage<Item>>(
            &client_for(&server),
            "/items",
            PageLimits::new(None),
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 3, "every page must be collected");
        assert_eq!(items[2].name, "c");
        assert!(!info.truncated, "a complete result is not truncated");
        assert_eq!(info.total, Some(3));
    }

    /// A limit smaller than the data must report itself as truncated, or the
    /// caller cannot tell a capped answer from a complete one.
    #[tokio::test]
    async fn a_limit_truncates_and_says_so() {
        let server = MockServer::start().await;
        let page_two = format!("{}/items?page=2", server.uri());

        Mock::given(method("GET"))
            .and(path_matcher("/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [{"name": "a"}, {"name": "b"}],
                "next": page_two
            })))
            .mount(&server)
            .await;

        let (items, info) = fetch_paged::<BitbucketPage<Item>>(
            &client_for(&server),
            "/items",
            PageLimits::new(Some(1)),
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(info.truncated, "a capped result must be labelled truncated");
        assert!(info.next.is_some(), "and must say where it stopped");
    }

    /// A server that always offers another page must not be followed forever.
    #[tokio::test]
    async fn the_budget_bounds_a_server_that_never_ends() {
        let server = MockServer::start().await;
        let forever = format!("{}/items?page=next", server.uri());

        Mock::given(method("GET"))
            .and(path_matcher("/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [{"name": "x"}],
                "next": forever
            })))
            .mount(&server)
            .await;

        let (items, info) = fetch_paged::<BitbucketPage<Item>>(
            &client_for(&server),
            "/items",
            PageLimits::new(None).with_budget(3),
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 3, "one item per allowed request");
        assert!(
            info.truncated,
            "budget exhaustion is truncation, not success"
        );
    }

    /// A cursor with no items would otherwise spin until the budget ran out.
    #[tokio::test]
    async fn an_empty_page_with_a_cursor_stops_the_walk() {
        let server = MockServer::start().await;
        let forever = format!("{}/items?page=next", server.uri());

        Mock::given(method("GET"))
            .and(path_matcher("/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [],
                "next": forever
            })))
            .mount(&server)
            .await;

        let (items, _) = fetch_paged::<BitbucketPage<Item>>(
            &client_for(&server),
            "/items",
            PageLimits::new(None).with_budget(20),
        )
        .await
        .unwrap();

        assert!(items.is_empty());
        let requests = server.received_requests().await.unwrap_or_default();
        assert_eq!(
            requests.len(),
            1,
            "must not keep asking: {}",
            requests.len()
        );
    }

    /// The offset walk end to end: three pages of an offset-paged endpoint,
    /// with `startAt` advancing and never accumulating.
    #[tokio::test]
    async fn an_offset_paged_endpoint_is_followed_to_the_end() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_matcher("/project/search"))
            .and(query_param("startAt", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [{"name": "c"}], "startAt": 2, "total": 3
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_matcher("/project/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [{"name": "a"}, {"name": "b"}], "startAt": 0, "total": 3
            })))
            .mount(&server)
            .await;

        let (items, info) = fetch_paged::<JiraOffsetPage<Item>>(
            &client_for(&server),
            "/project/search?expand=lead",
            PageLimits::new(None),
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 3, "every page collected");
        assert!(!info.truncated);
        assert_eq!(info.total, Some(3));

        for request in server.received_requests().await.unwrap_or_default() {
            let query = request.url.query().unwrap_or("");
            assert!(
                query.matches("startAt").count() <= 1,
                "offset accumulated: {query}"
            );
            assert!(
                query.contains("expand=lead"),
                "the original query must survive: {query}"
            );
        }
    }

    /// Jira's token goes back on the *original* path each time. Appending it to
    /// the previous request would put two `nextPageToken` values on page three.
    #[tokio::test]
    async fn a_jira_token_never_accumulates() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_matcher("/search"))
            .and(query_param("nextPageToken", "t2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{"name": "c"}], "isLast": true
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_matcher("/search"))
            .and(query_param("nextPageToken", "t1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{"name": "b"}], "nextPageToken": "t2", "isLast": false
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_matcher("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{"name": "a"}], "nextPageToken": "t1", "isLast": false
            })))
            .mount(&server)
            .await;

        let (items, info) = fetch_paged::<JiraPage<Item>>(
            &client_for(&server),
            "/search?jql=project%3DX",
            PageLimits::new(None),
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 3, "all three pages");
        assert!(!info.truncated);

        for request in server.received_requests().await.unwrap_or_default() {
            let query = request.url.query().unwrap_or("");
            assert!(
                query.matches("nextPageToken").count() <= 1,
                "token accumulated: {query}"
            );
            assert!(
                query.contains("jql=project"),
                "the original query must survive: {query}"
            );
        }
    }

    /// Following a cursor pointing at another host would leak credentials.
    /// `safe_join` rejects it at the client, and the error must surface.
    #[tokio::test]
    async fn a_foreign_next_url_is_refused() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_matcher("/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [{"name": "a"}],
                "next": "https://evil.example.com/2.0/items?page=2"
            })))
            .mount(&server)
            .await;

        let result = fetch_paged::<BitbucketPage<Item>>(
            &client_for(&server),
            "/items",
            PageLimits::new(None),
        )
        .await;

        assert!(
            result.is_err(),
            "a cross-origin cursor must not be followed"
        );
    }

    /// Jira reports no total, so the old `items.len() < total.unwrap_or(0)`
    /// check evaluated to false and a final page that overshot the limit was
    /// reported as complete while dropping rows.
    #[tokio::test]
    async fn overshooting_the_limit_on_a_final_page_is_still_truncation() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_matcher("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{"name": "a"}, {"name": "b"}, {"name": "c"}],
                "isLast": true
            })))
            .mount(&server)
            .await;

        let (items, info) = fetch_paged::<JiraPage<Item>>(
            &client_for(&server),
            "/search",
            PageLimits::new(Some(2)),
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 2);
        assert!(
            info.truncated,
            "a page that overshot the limit dropped rows and must say so"
        );
    }

    /// Exactly filling the limit with nothing left is complete, not truncated.
    #[tokio::test]
    async fn hitting_the_limit_exactly_is_not_truncation() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_matcher("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{"name": "a"}, {"name": "b"}],
                "isLast": true
            })))
            .mount(&server)
            .await;

        let (items, info) = fetch_paged::<JiraPage<Item>>(
            &client_for(&server),
            "/search",
            PageLimits::new(Some(2)),
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 2);
        assert!(!info.truncated, "nothing was dropped: {info:?}");
    }

    /// An abandoned walk is incomplete. bulk.rs refuses to delete on this flag,
    /// so reporting it as complete would let a partial list drive deletions.
    #[tokio::test]
    async fn an_abandoned_walk_is_reported_as_truncated() {
        let server = MockServer::start().await;
        let forever = format!("{}/items?page=next", server.uri());

        Mock::given(method("GET"))
            .and(path_matcher("/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "values": [],
                "next": forever
            })))
            .mount(&server)
            .await;

        let (_, info) = fetch_paged::<BitbucketPage<Item>>(
            &client_for(&server),
            "/items",
            PageLimits::new(None),
        )
        .await
        .unwrap();

        assert!(info.truncated, "the server said more exists: {info:?}");
    }

    /// A 200 whose body lacks the items key is malformed, not empty.
    #[tokio::test]
    async fn a_body_without_the_items_key_is_an_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_matcher("/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"page": 1})))
            .mount(&server)
            .await;

        let result = fetch_paged::<BitbucketPage<Item>>(
            &client_for(&server),
            "/items",
            PageLimits::new(None),
        )
        .await;

        assert!(
            result.is_err(),
            "a missing values key must not read as an empty result"
        );
    }

    /// `--limit 0` is documented as "everything". Passed through as `Some(0)`
    /// it returned nothing at all.
    #[test]
    fn a_zero_cli_limit_means_no_limit() {
        assert_eq!(PageLimits::from_cli_limit(0).limit, None);
        assert_eq!(PageLimits::from_cli_limit(25).limit, Some(25));
    }

    fn offset_page(value: serde_json::Value) -> JiraOffsetPage<Item> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_offset_page_advances_by_what_it_returned() {
        let page = offset_page(json!({
            "values": [{"name": "a"}, {"name": "b"}],
            "startAt": 0, "maxResults": 2, "total": 5
        }));
        let (items, cursor, total) = page.into_parts();
        assert_eq!(items.len(), 2);
        assert_eq!(
            cursor,
            Some(Continuation::Token {
                param: JIRA_START_AT_PARAM.to_string(),
                value: "2".to_string()
            })
        );
        assert_eq!(total, Some(5));
    }

    /// `isLast` wins over the arithmetic where the endpoint sends it.
    #[test]
    fn is_last_ends_an_offset_walk() {
        let page = offset_page(json!({
            "values": [{"name": "a"}], "startAt": 0, "total": 99, "isLast": true
        }));
        assert_eq!(page.into_parts().1, None);
    }

    #[test]
    fn reaching_the_total_ends_an_offset_walk() {
        let page = offset_page(json!({
            "values": [{"name": "a"}], "startAt": 4, "total": 5
        }));
        assert_eq!(page.into_parts().1, None);
    }

    /// Neither `isLast` nor `total`: an empty page must end the walk, or the
    /// offset would advance by zero forever.
    #[test]
    fn an_empty_offset_page_ends_the_walk() {
        let page = offset_page(json!({"values": [], "startAt": 10}));
        assert_eq!(page.into_parts().1, None);
    }

    #[test]
    fn append_query_adds_a_parameter() {
        assert_eq!(append_query("/x", "t", "1"), "/x?t=1");
        assert_eq!(append_query("/x?a=b", "t", "1"), "/x?a=b&t=1");
    }

    /// The bug this function exists to prevent: two cursors on page three.
    #[test]
    fn append_query_replaces_rather_than_duplicating() {
        let once = append_query("/x?a=b", "t", "1");
        let twice = append_query(&once, "t", "2");
        assert_eq!(twice, "/x?a=b&t=2");
        assert_eq!(twice.matches("t=").count(), 1);
    }

    #[test]
    fn append_query_encodes_its_value() {
        assert_eq!(append_query("/x", "t", "a b&c"), "/x?t=a%20b%26c");
    }
}
