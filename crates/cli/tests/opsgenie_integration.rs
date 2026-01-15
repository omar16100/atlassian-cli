use atlassian_cli_api::ApiClient;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_opsgenie_list_alerts() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/alerts"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "alert-123",
                    "tinyId": "1",
                    "message": "CPU usage high",
                    "status": "open",
                    "acknowledged": false,
                    "priority": "P1",
                    "createdAt": "2025-01-15T10:00:00.000Z"
                },
                {
                    "id": "alert-456",
                    "tinyId": "2",
                    "message": "Memory usage high",
                    "status": "open",
                    "acknowledged": true,
                    "priority": "P2",
                    "createdAt": "2025-01-15T11:00:00.000Z"
                }
            ],
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v2/alerts").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_opsgenie_get_alert() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/alerts/alert-123"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "id": "alert-123",
                "tinyId": "1",
                "message": "CPU usage high",
                "description": "Server CPU is at 95%",
                "status": "open",
                "acknowledged": false,
                "priority": "P1",
                "createdAt": "2025-01-15T10:00:00.000Z",
                "tags": ["production", "critical"],
                "source": "monitoring"
            },
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v2/alerts/alert-123").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["data"]["message"], "CPU usage high");
}

#[tokio::test]
async fn test_opsgenie_create_alert() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/alerts"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
            "result": "Request will be processed",
            "took": 0.004,
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let payload = serde_json::json!({
        "message": "New alert",
        "priority": "P1",
        "description": "This is a test alert"
    });

    let response: Result<serde_json::Value, _> = client.post("/v2/alerts", &payload).await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert!(result["requestId"].is_string());
}

#[tokio::test]
async fn test_opsgenie_acknowledge_alert() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/alerts/alert-123/acknowledge"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
            "result": "Request will be processed",
            "took": 0.003,
            "requestId": "req-456"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let payload = serde_json::json!({
        "user": "agent@example.com",
        "note": "Looking into this"
    });

    let response: Result<serde_json::Value, _> = client
        .post("/v2/alerts/alert-123/acknowledge", &payload)
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_opsgenie_close_alert() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/alerts/alert-123/close"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
            "result": "Request will be processed",
            "took": 0.003,
            "requestId": "req-789"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let payload = serde_json::json!({
        "user": "agent@example.com",
        "note": "Issue resolved"
    });

    let response: Result<serde_json::Value, _> =
        client.post("/v2/alerts/alert-123/close", &payload).await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_opsgenie_list_incidents() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/incidents"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "inc-123",
                    "tinyId": "1",
                    "message": "Production outage",
                    "status": "open",
                    "priority": "P1",
                    "createdAt": "2025-01-15T10:00:00.000Z"
                }
            ],
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v1/incidents").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_opsgenie_create_incident() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/incidents/create"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
            "result": "Request will be processed",
            "took": 0.005,
            "requestId": "req-abc"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let payload = serde_json::json!({
        "message": "Production outage",
        "priority": "P1",
        "description": "Database is down"
    });

    let response: Result<serde_json::Value, _> =
        client.post("/v1/incidents/create", &payload).await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_opsgenie_list_schedules() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/schedules"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "sched-123",
                    "name": "Primary On-Call",
                    "description": "Main on-call rotation",
                    "enabled": true,
                    "timezone": "America/New_York"
                },
                {
                    "id": "sched-456",
                    "name": "Secondary On-Call",
                    "description": "Backup on-call rotation",
                    "enabled": true,
                    "timezone": "America/New_York"
                }
            ],
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v2/schedules").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_opsgenie_get_on_call() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/schedules/sched-123/on-calls"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "onCallParticipants": [
                    {
                        "id": "user-123",
                        "name": "John Doe",
                        "type": "user"
                    }
                ]
            },
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> =
        client.get("/v2/schedules/sched-123/on-calls").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert!(result["data"]["onCallParticipants"].is_array());
}

#[tokio::test]
async fn test_opsgenie_list_teams() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/teams"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "team-123",
                    "name": "Platform Team",
                    "description": "Platform engineering team"
                },
                {
                    "id": "team-456",
                    "name": "SRE Team",
                    "description": "Site reliability engineering"
                }
            ],
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v2/teams").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_opsgenie_list_services() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/services"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "svc-123",
                    "name": "API Service",
                    "description": "Main API service"
                }
            ],
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v1/services").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_opsgenie_list_heartbeats() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/heartbeats"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "name": "api-health",
                    "description": "API health check",
                    "interval": 5,
                    "intervalUnit": "minutes",
                    "enabled": true,
                    "expired": false
                }
            ],
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v2/heartbeats").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_opsgenie_ping_heartbeat() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/heartbeats/api-health/ping"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
            "result": "Ping successful",
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v2/heartbeats/api-health/ping").await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_opsgenie_list_escalations() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/escalations"))
        .and(header("Authorization", "GenieKey test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "esc-123",
                    "name": "Default Escalation",
                    "description": "Default escalation policy"
                }
            ],
            "requestId": "req-123"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_genie_key("test-api-key");

    let response: Result<serde_json::Value, _> = client.get("/v2/escalations").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["data"].as_array().unwrap().len(), 1);
}
