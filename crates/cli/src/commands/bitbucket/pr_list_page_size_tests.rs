//! `bb pr list` must not ask for more than 50 pull requests in one page.
//!
//! The general Bitbucket page size limit is 100, and `page_size` uses it. The
//! pull request list refuses a `pagelen` above 50 with "Invalid pagelen", so
//! `--limit 0` and each `--limit` above 50 failed. The mock here refuses those
//! values in the same way.

use atlassian_cli_api::ApiClient;
use atlassian_cli_output::{OutputFormat, OutputRenderer};
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::pullrequests::list_pull_requests;
use super::utils::BitbucketContext;

const LIST: &str = "/2.0/repositories/myworkspace/web_app/pullrequests";

fn pull_requests(first: i64, count: i64) -> Vec<serde_json::Value> {
    (first..first + count)
        .map(|id| {
            json!({
                "id": id,
                "title": format!("Change {id}"),
                "state": "OPEN",
                "author": {"display_name": "Dev User"},
                "source": {"branch": {"name": "feature"}},
                "destination": {"branch": {"name": "main"}}
            })
        })
        .collect()
}

/// A pull request list that has `total` rows and gives them in pages of the
/// size that the request asks for. Each `pagelen` that is not in `accepted`
/// gets the answer of Bitbucket for a value above 50.
async fn server(total: i64, accepted: &[i64]) -> MockServer {
    let server = MockServer::start().await;
    for pagelen in accepted {
        let first_count = (*pagelen).min(total);
        let mut first = json!({"pagelen": pagelen, "values": pull_requests(1, first_count)});
        if total > first_count {
            first["next"] = json!(format!(
                "{}{LIST}?state=OPEN&pagelen={pagelen}&page=2",
                server.uri()
            ));
        }
        Mock::given(method("GET"))
            .and(path(LIST))
            .and(query_param("pagelen", pagelen.to_string()))
            .and(query_param_is_missing("page"))
            .respond_with(ResponseTemplate::new(200).set_body_json(first))
            .expect(1)
            .mount(&server)
            .await;
        if total > first_count {
            let rest = (total - first_count).min(*pagelen);
            Mock::given(method("GET"))
                .and(path(LIST))
                .and(query_param("pagelen", pagelen.to_string()))
                .and(query_param("page", "2"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "pagelen": pagelen,
                    "values": pull_requests(first_count + 1, rest)
                })))
                .expect(1)
                .mount(&server)
                .await;
        }
    }
    // Every other request, for example `pagelen=100`.
    Mock::given(method("GET"))
        .and(path(LIST))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "type": "error",
            "error": {"message": "Invalid pagelen"}
        })))
        .expect(0)
        .mount(&server)
        .await;
    server
}

async fn list(server: &MockServer, limit: usize) -> anyhow::Result<()> {
    let renderer = OutputRenderer::new(OutputFormat::Quiet);
    let ctx = BitbucketContext {
        client: ApiClient::new(server.uri()).unwrap(),
        renderer: &renderer,
        is_bearer: false,
    };
    list_pull_requests(&ctx, "myworkspace", "web_app", "OPEN", limit).await
}

/// The reported case: `--limit 0` sent `pagelen=100`.
#[tokio::test]
async fn limit_zero_asks_for_pages_of_50_and_follows_the_next_page() {
    let server = server(60, &[50]).await;
    list(&server, 0)
        .await
        .expect("--limit 0 must return all rows");
}

#[tokio::test]
async fn a_limit_above_50_asks_for_pages_of_50_and_follows_the_next_page() {
    for limit in [51, 100] {
        let server = server(60, &[50]).await;
        list(&server, limit)
            .await
            .unwrap_or_else(|e| panic!("--limit {limit}: {e:#}"));
    }
}

/// Unchanged: a limit of 50 or less is the page size, in one request.
#[tokio::test]
async fn a_limit_of_50_or_less_is_one_request_of_that_size() {
    for limit in [50, 25, 1] {
        let server = server(limit as i64, &[limit as i64]).await;
        list(&server, limit)
            .await
            .unwrap_or_else(|e| panic!("--limit {limit}: {e:#}"));
    }
}
