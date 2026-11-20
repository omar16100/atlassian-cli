//! Pipeline commands against a mock Bitbucket, checked by the requests they make.

use atlassian_cli_api::ApiClient;
use atlassian_cli_output::{OutputFormat, OutputRenderer};
use serde_json::json;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::utils::BitbucketContext;
use super::logs::get_pipeline_logs;
use super::steps::pipeline_has_failed_steps;
use super::watch::watch_pipeline;
use super::{get_pipeline, status_to_exit_code};

const PIPELINE: &str = "{11111111-2222-3333-4444-555555555555}";

fn ctx<'a>(server: &MockServer, renderer: &'a OutputRenderer) -> BitbucketContext<'a> {
    BitbucketContext {
        client: ApiClient::new(server.uri()).unwrap(),
        renderer,
        is_bearer: false,
    }
}

fn paused_pipeline() -> serde_json::Value {
    json!({
        "uuid": PIPELINE,
        "build_number": 592,
        "state": {
            "name": "IN_PROGRESS",
            "type": "pipeline_state_in_progress",
            "stage": {"name": "PAUSED", "type": "pipeline_state_in_progress_paused"}
        },
        "created_on": "2026-09-17T02:00:00Z",
        "completed_on": null,
        "target": {"ref_name": "main", "type": "pipeline_ref_target", "commit": {"hash": "0123456789"}}
    })
}

fn steps_page(values: serde_json::Value) -> serde_json::Value {
    json!({"values": values, "pagelen": 100})
}

fn completed_step(uuid: &str, name: &str, result: &str) -> serde_json::Value {
    json!({
        "uuid": uuid,
        "name": name,
        "state": {"name": "COMPLETED", "result": {"name": result}},
        "trigger": {"type": "pipeline_step_trigger_automatic"}
    })
}

async fn mount_pipeline(server: &MockServer, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/2\.0/repositories/ws/web_app/pipelines/[^/]+$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// `pipeline get` on a paused build fetches its steps, once, to count the
/// manual ones it waits on, even without `--steps`.
#[tokio::test]
async fn get_fetches_steps_of_a_paused_build_to_count_them() {
    let server = MockServer::start().await;
    mount_pipeline(&server, paused_pipeline()).await;
    Mock::given(method("GET"))
        .and(path_regex(r"/steps/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(steps_page(json!([
            completed_step("{s1}", "build", "SUCCESSFUL"),
            {
                "uuid": "{s2}",
                "name": "deploy-prod",
                "state": {"name": "PENDING", "type": "pipeline_step_state_pending"},
                "trigger": {"type": "pipeline_step_trigger_manual"}
            }
        ]))))
        .expect(1)
        .mount(&server)
        .await;

    let renderer = OutputRenderer::new(OutputFormat::Json);
    get_pipeline(&ctx(&server, &renderer), "ws", "web_app", PIPELINE, false)
        .await
        .unwrap();
}

/// A running build costs no extra request.
#[tokio::test]
async fn get_does_not_fetch_steps_of_a_running_build() {
    let server = MockServer::start().await;
    let mut running = paused_pipeline();
    running["state"]["stage"] = json!({"name": "RUNNING"});
    mount_pipeline(&server, running).await;
    Mock::given(method("GET"))
        .and(path_regex(r"/steps/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(steps_page(json!([]))))
        .expect(0)
        .mount(&server)
        .await;

    let renderer = OutputRenderer::new(OutputFormat::Json);
    get_pipeline(&ctx(&server, &renderer), "ws", "web_app", PIPELINE, false)
        .await
        .unwrap();
}

/// `watch` used to poll a paused build until `--timeout`, or forever.
#[tokio::test]
async fn watch_stops_on_a_paused_build_with_exit_code_three() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/2\.0/repositories/ws/web_app/pipelines/[^/]+$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(paused_pipeline()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"/steps/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(steps_page(json!([]))))
        .mount(&server)
        .await;

    let renderer = OutputRenderer::new(OutputFormat::Json);
    let status = watch_pipeline(
        &ctx(&server, &renderer),
        "ws",
        "web_app",
        PIPELINE,
        60,
        false,
        None,
        None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(status, "PAUSED");
    assert_eq!(status_to_exit_code(&status), 3);
}

/// A failed step reads `COMPLETED` in `state.name`; only `result` says it
/// failed. `rerun --pr --failed-only` depends on this answer.
#[tokio::test]
async fn a_completed_step_with_a_failed_result_counts_as_a_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"/steps/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(steps_page(json!([
            completed_step("{s1}", "build", "SUCCESSFUL"),
            completed_step("{s2}", "test", "FAILED"),
        ]))))
        .mount(&server)
        .await;

    let renderer = OutputRenderer::new(OutputFormat::Json);
    let failed = pipeline_has_failed_steps(&ctx(&server, &renderer), "ws", "web_app", PIPELINE)
        .await
        .unwrap();
    assert!(failed);
}

/// `logs --failed-only` fetches the failed step's log and no other.
#[tokio::test]
async fn logs_failed_only_fetches_only_the_failed_step() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"/steps/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(steps_page(json!([
            completed_step("s-ok", "build", "SUCCESSFUL"),
            completed_step("s-bad", "test", "FAILED"),
        ]))))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"/steps/s-bad/log$"))
        .respond_with(ResponseTemplate::new(200).set_body_string("assertion failed\n"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"/steps/s-ok/log$"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok\n"))
        .expect(0)
        .mount(&server)
        .await;

    let renderer = OutputRenderer::new(OutputFormat::Json);
    get_pipeline_logs(
        &ctx(&server, &renderer),
        "ws",
        "web_app",
        PIPELINE,
        None,
        None,
        None,
        false,
        true,
    )
    .await
    .unwrap();
}

/// `--since` and `--before` go into the `q` filter in one fixed form: a
/// numeric offset, never `Z`, and the fraction that the caller gave.
#[tokio::test]
async fn list_sends_the_parsed_time_expressions_in_the_query() {
    use super::super::time_parser::parse_time_expression;
    use super::list::list_pipelines;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/2\.0/repositories/ws/web_app/pipelines/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"values": []})))
        .expect(1)
        .mount(&server)
        .await;

    let since = parse_time_expression("2024-01-01").unwrap();
    let before = parse_time_expression("2024-06-01T08:30:00.5+08:00").unwrap();
    let renderer = OutputRenderer::new(OutputFormat::Json);
    list_pipelines(
        &ctx(&server, &renderer),
        "ws",
        "web_app",
        10,
        None,
        None,
        None,
        Some(&since),
        Some(&before),
        false,
        false,
    )
    .await
    .unwrap();

    let requests = server.received_requests().await.unwrap();
    let filter = requests[0]
        .url
        .query_pairs()
        .find(|(name, _)| name == "q")
        .map(|(_, value)| value.into_owned())
        .expect("the request must have a q filter");
    assert!(
        filter.contains(r#"created_on>="2024-01-01T00:00:00+00:00""#),
        "filter: {filter}"
    );
    assert!(
        filter.contains(r#"created_on<"2024-06-01T08:30:00.500+08:00""#),
        "filter: {filter}"
    );
}

/// A step that runs has no `duration_in_seconds`. The duration is calculated
/// from `started_on` and the clock.
#[tokio::test]
async fn steps_show_a_calculated_duration_only_for_a_running_step() {
    use super::steps::fetch_steps;

    let running = |uuid: &str, started_on: &str| {
        json!({
            "uuid": uuid,
            "name": uuid,
            "state": {"name": "IN_PROGRESS"},
            "started_on": started_on
        })
    };
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"/steps/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(steps_page(json!([
            // A start time in the future gives zero.
            running("{future}", "9999-12-31T23:59:59.123456+00:00"),
            running("{past}", "2000-01-01T00:00:00+08:00"),
            running("{invalid}", "2000-01-01 00:00"),
            {
                "uuid": "{done}",
                "name": "done",
                "state": {"name": "COMPLETED", "result": {"name": "SUCCESSFUL"}},
                "started_on": "2000-01-01T00:00:00Z",
                "completed_on": "2000-01-01T00:01:05Z",
                "duration_in_seconds": 65
            }
        ]))))
        .mount(&server)
        .await;

    let renderer = OutputRenderer::new(OutputFormat::Json);
    let steps = fetch_steps(&ctx(&server, &renderer), "ws", "web_app", PIPELINE, true)
        .await
        .unwrap();
    let duration = |uuid: &str| {
        steps
            .iter()
            .find(|step| step.uuid == uuid)
            .unwrap()
            .duration
            .clone()
    };

    assert_eq!(duration("{future}").as_deref(), Some("~0s"));
    let past = duration("{past}").expect("a running step must have a duration");
    assert!(past.starts_with('~') && past.contains("h ") && past.ends_with('s'));
    // From the year 2000 to the present is more than 200000 hours.
    let hours: u64 = past[1..past.find('h').unwrap()].parse().unwrap();
    assert!(hours > 200_000, "duration: {past}");
    assert_eq!(duration("{invalid}"), None);
    assert_eq!(duration("{done}").as_deref(), Some("1m 5s"));
}
