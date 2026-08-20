use atlassian_cli_api::ApiClient;
use wiremock::matchers::{body_string_contains, header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_jira_search_issues() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/search/jql"))
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

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/rest/api/3/search/jql?jql=project%20%3D%20TEST&maxResults=50")
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_jira_search_410_returns_endpoint_gone() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/search"))
        .respond_with(ResponseTemplate::new(410).set_body_json(serde_json::json!({
            "errorMessages": ["The requested API has been removed. Please migrate to the /rest/api/3/search/jql API."]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/rest/api/3/search?jql=project%20%3D%20TEST")
        .await;

    assert!(response.is_err());
    let err = response.unwrap_err();
    assert!(
        matches!(err, atlassian_cli_api::error::ApiError::EndpointGone { .. }),
        "Expected EndpointGone, got: {err:?}"
    );
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
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

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/3/issue/NOTFOUND-999").await;

    assert!(response.is_err());
}

// ---------------------------------------------------------------------------
// Attachments (issue #93)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_jira_list_issue_attachments() {
    let mock_server = MockServer::start().await;

    // Jira returns `id` as a number in some responses and a string in others.
    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/TEST-1"))
        .and(query_param("fields", "attachment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "TEST-1",
            "fields": {
                "attachment": [
                    {
                        "id": 10001,
                        "filename": "diagram.png",
                        "mimeType": "image/png",
                        "size": 98150,
                        "content": "https://x/rest/api/3/attachment/content/10001",
                        "author": {"displayName": "Ada Lovelace"},
                        "created": "2026-08-01T10:00:00.000+0000"
                    },
                    {
                        "id": "10002",
                        "filename": "notes.txt",
                        "mimeType": "text/plain",
                        "size": 12
                    }
                ]
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: serde_json::Value = client
        .get("/rest/api/3/issue/TEST-1?fields=attachment")
        .await
        .unwrap();

    let attachments = response["fields"]["attachment"].as_array().unwrap();
    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[0]["filename"], "diagram.png");
    assert_eq!(attachments[1]["id"], "10002");
}

#[tokio::test]
async fn test_jira_get_attachment_metadata() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/attachment/10001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "10001",
            "filename": "diagram.png",
            "mimeType": "image/png",
            "size": 98150,
            "content": "https://x/rest/api/3/attachment/content/10001"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let attachment: serde_json::Value = client.get("/rest/api/3/attachment/10001").await.unwrap();

    assert_eq!(attachment["filename"], "diagram.png");
    assert_eq!(attachment["size"], 98150);
}

/// The content endpoint answers with a 302 to a signed URL on Atlassian's media
/// host. Two MockServers bind different ports on 127.0.0.1, which reqwest treats
/// as cross-origin, so this reproduces the production topology.
#[tokio::test]
async fn test_jira_download_attachment_follows_302_to_media_host() {
    let media = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/file/abc/binary"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"PNGDATA".to_vec()))
        .mount(&media)
        .await;

    let jira = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/attachment/content/10001"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "location",
            format!("{}/file/abc/binary?token=xyz", media.uri()).as_str(),
        ))
        .mount(&jira)
        .await;

    let client = ApiClient::new(jira.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let bytes = client
        .get_bytes("/rest/api/3/attachment/content/10001")
        .await
        .unwrap();

    assert_eq!(bytes, b"PNGDATA".to_vec());
}

/// Regression guard for the security claim behind the download design: reqwest
/// must strip `Authorization` on the cross-host redirect, because the media URL
/// carries its own short-lived token and our site credential has no business
/// leaving the site's origin. A leak yields 401, which is not retryable, so this
/// test fails hard rather than flaking.
#[tokio::test]
async fn test_jira_download_does_not_leak_credentials_across_redirect() {
    let media = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/file/abc/binary"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(1)
        // Verified on drop: no credential-bearing request may reach the media host.
        .expect(0)
        .mount(&media)
        .await;
    Mock::given(method("GET"))
        .and(path("/file/abc/binary"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"PNGDATA".to_vec()))
        .with_priority(2)
        .mount(&media)
        .await;

    let jira = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/attachment/content/10001"))
        // Requiring auth here keeps the test honest: without it, the assertion
        // below would also pass if we simply stopped sending credentials at all.
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "location",
            format!("{}/file/abc/binary", media.uri()).as_str(),
        ))
        .mount(&jira)
        .await;

    let client = ApiClient::new(jira.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let bytes = client
        .get_bytes("/rest/api/3/attachment/content/10001")
        .await
        .expect("credentials leaked to the redirect target");

    assert_eq!(bytes, b"PNGDATA".to_vec());
}

#[tokio::test]
async fn test_jira_download_attachment_404_maps_to_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/attachment/content/999"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let err = client
        .get_bytes("/rest/api/3/attachment/content/999")
        .await
        .unwrap_err();

    assert!(
        matches!(err, atlassian_cli_api::error::ApiError::NotFound { .. }),
        "got {err:?}"
    );
}

/// The exact symptom reported in #93: fetching the attachment URL without the
/// right credentials is rejected.
#[tokio::test]
async fn test_jira_attachment_content_403_maps_to_forbidden() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/attachment/content/10001"))
        .respond_with(ResponseTemplate::new(403).set_body_string("no permission"))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let err = client
        .get_bytes("/rest/api/3/attachment/content/10001")
        .await
        .unwrap_err();

    assert!(
        matches!(err, atlassian_cli_api::error::ApiError::Forbidden { .. }),
        "got {err:?}"
    );
}

/// Mirrors how `upload_attachments` builds its request: the field name must be
/// `file` and Jira requires the XSRF opt-out header.
#[tokio::test]
async fn test_jira_upload_attachment_multipart() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/3/issue/TEST-1/attachments"))
        .and(header("X-Atlassian-Token", "no-check"))
        .and(body_string_contains("name=\"file\""))
        .and(body_string_contains("filename=\"note.txt\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "10005", "filename": "note.txt", "size": 5, "mimeType": "text/plain"}
        ])))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec()).file_name("note.txt"),
    );
    let base_url = client.base_url().trim_end_matches('/').to_string();
    let request = client
        .http_client()
        .post(format!("{base_url}/rest/api/3/issue/TEST-1/attachments"))
        .multipart(form)
        .header("X-Atlassian-Token", "no-check");
    let response = client.apply_auth(request).send().await.unwrap();

    assert!(response.status().is_success());
    let uploaded: serde_json::Value = response.json().await.unwrap();
    assert_eq!(uploaded[0]["id"], "10005");
}

#[tokio::test]
async fn test_jira_upload_multiple_files_sends_repeated_file_parts() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/3/issue/TEST-1/attachments"))
        .and(body_string_contains("filename=\"a.txt\""))
        .and(body_string_contains("filename=\"b.txt\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "1", "filename": "a.txt"},
            {"id": "2", "filename": "b.txt"}
        ])))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"a".to_vec()).file_name("a.txt"),
        )
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"b".to_vec()).file_name("b.txt"),
        );
    let base_url = client.base_url().trim_end_matches('/').to_string();
    let request = client
        .http_client()
        .post(format!("{base_url}/rest/api/3/issue/TEST-1/attachments"))
        .multipart(form)
        .header("X-Atlassian-Token", "no-check");
    let response = client.apply_auth(request).send().await.unwrap();

    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_jira_delete_attachment_204() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/rest/api/3/attachment/10001"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    client
        .delete_no_content("/rest/api/3/attachment/10001")
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Comments and transitions (issues #100, #101)
// ---------------------------------------------------------------------------

/// Regression for #100. The old code PUT to `/rest/api/3/comment/{id}`, which
/// Jira Cloud does not have, so every update 404'd. Only the URL can catch this:
/// a mock will answer whatever path it is handed, so the assertion has to be the
/// path itself.
#[tokio::test]
async fn test_jira_update_comment_uses_the_issue_scoped_path() {
    let mock_server = MockServer::start().await;

    // The route that does not exist. If the command regresses to it, the
    // issue-scoped mock below never matches and the test fails.
    Mock::given(method("PUT"))
        .and(path("/rest/api/3/comment/10100"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&mock_server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/rest/api/3/issue/TEST-1/comment/10100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "10100"})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: serde_json::Value = client
        .put(
            "/rest/api/3/issue/TEST-1/comment/10100",
            &serde_json::json!({"body": {}}),
        )
        .await
        .unwrap();

    assert_eq!(response["id"], "10100");
}

#[tokio::test]
async fn test_jira_delete_comment_uses_the_issue_scoped_path() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/rest/api/3/comment/10100"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&mock_server)
        .await;

    Mock::given(method("DELETE"))
        .and(path("/rest/api/3/issue/TEST-1/comment/10100"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    client
        .delete_no_content("/rest/api/3/issue/TEST-1/comment/10100")
        .await
        .unwrap();
}

/// #101: the transition list, including the `to` status a transition lands in.
#[tokio::test]
async fn test_jira_list_transitions() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/TEST-1/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transitions": [
                {"id": "11", "name": "To Do", "to": {"name": "To Do"}},
                {"id": "21", "name": "In Progress", "to": {"name": "In Progress"}},
                // Older instances omit `to`; it must not abort the parse.
                {"id": "31", "name": "Done"}
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: serde_json::Value = client
        .get("/rest/api/3/issue/TEST-1/transitions")
        .await
        .unwrap();

    let transitions = response["transitions"].as_array().unwrap();
    assert_eq!(transitions.len(), 3);
    assert_eq!(transitions[1]["id"], "21");
    assert!(transitions[2]["to"].is_null());
}
