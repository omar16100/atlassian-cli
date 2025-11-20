use atlassian_cli_api::ApiClient;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_jira_search_issues() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/search"))
        .and(query_param("jql", "project = TEST"))
        .and(query_param("maxResults", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total": 2,
            "issues": [
                {
                    "id": "10001",
                    "key": "TEST-1",
                    "fields": {
                        "summary": "First test issue",
                        "status": {"name": "Open"},
                        "assignee": {"displayName": "John Doe"},
                        "created": "2025-01-01T10:00:00.000+0000"
                    }
                },
                {
                    "id": "10002",
                    "key": "TEST-2",
                    "fields": {
                        "summary": "Second test issue",
                        "status": {"name": "In Progress"},
                        "assignee": null,
                        "created": "2025-01-02T11:00:00.000+0000"
                    }
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/rest/api/3/search?jql=project%20%3D%20TEST&maxResults=50")
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_jira_get_issue() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/TEST-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "10001",
            "key": "TEST-123",
            "fields": {
                "summary": "Test issue summary",
                "description": "Test issue description",
                "status": {"name": "Open"},
                "assignee": {"displayName": "Jane Doe", "accountId": "123abc"},
                "priority": {"name": "High"},
                "created": "2025-01-01T10:00:00.000+0000",
                "updated": "2025-01-15T15:30:00.000+0000"
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/3/issue/TEST-123").await;

    assert!(response.is_ok());
    let issue = response.unwrap();
    assert_eq!(issue["key"], "TEST-123");
    assert_eq!(issue["fields"]["summary"], "Test issue summary");
}

#[tokio::test]
async fn test_jira_create_issue() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/3/issue"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "10003",
            "key": "TEST-124",
            "self": format!("{}/rest/api/3/issue/10003", mock_server.uri())
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "fields": {
            "project": {"key": "TEST"},
            "summary": "New test issue",
            "issuetype": {"name": "Task"}
        }
    });

    let response: Result<serde_json::Value, _> = client.post("/rest/api/3/issue", &payload).await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["key"], "TEST-124");
}

#[tokio::test]
async fn test_jira_update_issue() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/rest/api/3/issue/TEST-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "fields": {
            "summary": "Updated summary",
            "description": "Updated description"
        }
    });

    let response: Result<serde_json::Value, _> =
        client.put("/rest/api/3/issue/TEST-123", &payload).await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_jira_delete_issue() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/rest/api/3/issue/TEST-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.delete("/rest/api/3/issue/TEST-123").await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_jira_transition_issue() {
    let mock_server = MockServer::start().await;

    // First mock to get available transitions
    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/TEST-123/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transitions": [
                {"id": "11", "name": "To Do", "to": {"name": "To Do"}},
                {"id": "21", "name": "In Progress", "to": {"name": "In Progress"}},
                {"id": "31", "name": "Done", "to": {"name": "Done"}}
            ]
        })))
        .mount(&mock_server)
        .await;

    // Mock the transition POST
    Mock::given(method("POST"))
        .and(path("/rest/api/3/issue/TEST-123/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    // Get transitions
    let response: Result<serde_json::Value, _> =
        client.get("/rest/api/3/issue/TEST-123/transitions").await;
    assert!(response.is_ok());

    // Perform transition
    let payload = serde_json::json!({"transition": {"id": "21"}});
    let response: Result<serde_json::Value, _> = client
        .post("/rest/api/3/issue/TEST-123/transitions", &payload)
        .await;
    assert!(response.is_ok());
}

#[tokio::test]
async fn test_jira_list_projects() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": "10000",
                "key": "TEST",
                "name": "Test Project",
                "projectTypeKey": "software",
                "lead": {"displayName": "Admin User"}
            },
            {
                "id": "10001",
                "key": "DEMO",
                "name": "Demo Project",
                "projectTypeKey": "business",
                "lead": {"displayName": "Demo Lead"}
            }
        ])))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/3/project").await;

    assert!(response.is_ok());
    let projects = response.unwrap();
    assert!(projects.is_array());
    assert_eq!(projects.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_jira_create_component() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/3/component"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "10100",
            "name": "Backend",
            "project": "TEST",
            "description": "Backend component"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "name": "Backend",
        "project": "TEST",
        "description": "Backend component"
    });

    let response: Result<serde_json::Value, _> =
        client.post("/rest/api/3/component", &payload).await;

    assert!(response.is_ok());
    let component = response.unwrap();
    assert_eq!(component["name"], "Backend");
}

#[tokio::test]
async fn test_jira_audit_list() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/auditing/record"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "records": [
                {
                    "id": 1,
                    "summary": "User logged in",
                    "category": "user",
                    "objectItem": {
                        "typeName": "USER",
                        "name": "admin"
                    },
                    "authorKey": "admin",
                    "created": "2025-01-20T10:00:00.000+0000"
                },
                {
                    "id": 2,
                    "summary": "Issue created",
                    "category": "issue",
                    "objectItem": {
                        "typeName": "ISSUE",
                        "name": "TEST-123"
                    },
                    "authorKey": "user1",
                    "created": "2025-01-20T11:00:00.000+0000"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/3/auditing/record").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["records"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_jira_list_webhooks() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/webhooks/1.0/webhook"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": 1,
                "name": "Test Webhook",
                "url": "https://example.com/webhook",
                "enabled": true,
                "events": ["jira:issue_created", "jira:issue_updated"]
            }
        ])))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/webhooks/1.0/webhook").await;

    assert!(response.is_ok());
    let webhooks = response.unwrap();
    assert!(webhooks.is_array());
}

#[tokio::test]
async fn test_jira_error_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/NOTFOUND-999"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "errorMessages": ["Issue does not exist or you do not have permission to see it."],
            "errors": {}
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/3/issue/NOTFOUND-999").await;

    assert!(response.is_err());
}
