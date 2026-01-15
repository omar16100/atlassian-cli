use atlassian_cli_api::ApiClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_jsm_list_service_desks() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/servicedeskapi/servicedesk"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "size": 2,
            "start": 0,
            "limit": 25,
            "isLastPage": true,
            "values": [
                {
                    "id": "1",
                    "projectId": "10001",
                    "projectKey": "SUPPORT",
                    "projectName": "Support Desk"
                },
                {
                    "id": "2",
                    "projectId": "10002",
                    "projectKey": "IT",
                    "projectName": "IT Help Desk"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/servicedeskapi/servicedesk").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_jsm_get_service_desk() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/servicedeskapi/servicedesk/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "1",
            "projectId": "10001",
            "projectKey": "SUPPORT",
            "projectName": "Support Desk"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/servicedeskapi/servicedesk/1").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["projectKey"], "SUPPORT");
}

#[tokio::test]
async fn test_jsm_list_requests() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/servicedeskapi/servicedesk/1/request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "size": 2,
            "start": 0,
            "limit": 25,
            "isLastPage": true,
            "values": [
                {
                    "issueId": "10001",
                    "issueKey": "SUPPORT-1",
                    "requestTypeId": "1",
                    "currentStatus": {"status": "Waiting for Support"}
                },
                {
                    "issueId": "10002",
                    "issueKey": "SUPPORT-2",
                    "requestTypeId": "2",
                    "currentStatus": {"status": "Resolved"}
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/rest/servicedeskapi/servicedesk/1/request")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_jsm_create_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/servicedeskapi/servicedesk/1/request"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "issueId": "10003",
            "issueKey": "SUPPORT-3",
            "requestTypeId": "1",
            "currentStatus": {"status": "Waiting for Support"}
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "serviceDeskId": "1",
        "requestTypeId": "1",
        "requestFieldValues": {
            "summary": "Test request",
            "description": "Test description"
        }
    });

    let response: Result<serde_json::Value, _> = client
        .post("/rest/servicedeskapi/servicedesk/1/request", &payload)
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["issueKey"], "SUPPORT-3");
}

#[tokio::test]
async fn test_jsm_get_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/servicedeskapi/request/SUPPORT-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issueId": "10001",
            "issueKey": "SUPPORT-1",
            "requestTypeId": "1",
            "serviceDeskId": "1",
            "createdDate": {"iso8601": "2025-01-15T10:00:00+0000"},
            "reporter": {"displayName": "John Doe"},
            "currentStatus": {"status": "Waiting for Support"}
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/servicedeskapi/request/SUPPORT-1").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["issueKey"], "SUPPORT-1");
}

#[tokio::test]
async fn test_jsm_list_request_types() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/servicedeskapi/servicedesk/1/requesttype"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "size": 2,
            "start": 0,
            "limit": 25,
            "isLastPage": true,
            "values": [
                {
                    "id": "1",
                    "name": "IT Support",
                    "description": "General IT support",
                    "serviceDeskId": "1"
                },
                {
                    "id": "2",
                    "name": "Hardware Request",
                    "description": "Request new hardware",
                    "serviceDeskId": "1"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/rest/servicedeskapi/servicedesk/1/requesttype")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_jsm_list_customers() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/servicedeskapi/servicedesk/1/customer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "size": 2,
            "start": 0,
            "limit": 25,
            "isLastPage": true,
            "values": [
                {
                    "accountId": "acc123",
                    "displayName": "John Doe",
                    "emailAddress": "john@example.com"
                },
                {
                    "accountId": "acc456",
                    "displayName": "Jane Smith",
                    "emailAddress": "jane@example.com"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/rest/servicedeskapi/servicedesk/1/customer")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_jsm_add_comment() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/servicedeskapi/request/SUPPORT-1/comment"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "1001",
            "body": "Test comment",
            "public": true,
            "author": {"displayName": "Agent"},
            "created": {"iso8601": "2025-01-15T10:00:00+0000"}
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "body": "Test comment",
        "public": true
    });

    let response: Result<serde_json::Value, _> = client
        .post("/rest/servicedeskapi/request/SUPPORT-1/comment", &payload)
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["body"], "Test comment");
}

#[tokio::test]
async fn test_jsm_list_request_transitions() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/servicedeskapi/request/SUPPORT-1/transition"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "size": 2,
            "start": 0,
            "limit": 25,
            "isLastPage": true,
            "values": [
                {
                    "id": "11",
                    "name": "Resolve"
                },
                {
                    "id": "21",
                    "name": "Close"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/rest/servicedeskapi/request/SUPPORT-1/transition")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_jsm_perform_transition() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/servicedeskapi/request/SUPPORT-1/transition"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "id": "11",
        "additionalComment": {"body": "Resolved the issue"}
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/rest/servicedeskapi/request/SUPPORT-1/transition",
            &payload,
        )
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_jsm_list_queues() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/servicedeskapi/servicedesk/1/queue"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "size": 2,
            "start": 0,
            "limit": 25,
            "isLastPage": true,
            "values": [
                {
                    "id": "1",
                    "name": "Open Issues",
                    "issueCount": 10
                },
                {
                    "id": "2",
                    "name": "Waiting for Customer",
                    "issueCount": 5
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/servicedeskapi/servicedesk/1/queue").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}
