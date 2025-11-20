use atlassian_cli_api::ApiClient;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ============================================================================
// Space Operations Tests
// ============================================================================

#[tokio::test]
async fn test_list_spaces() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/spaces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "id": "123456",
                    "key": "DOCS",
                    "name": "Documentation",
                    "type": "global",
                    "status": "current"
                },
                {
                    "id": "789012",
                    "key": "TEAM",
                    "name": "Team Space",
                    "type": "global",
                    "status": "current"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/wiki/api/v2/spaces").await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["results"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_get_space() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/spaces"))
        .and(query_param("keys", "DOCS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{
                "id": "123456",
                "key": "DOCS",
                "name": "Documentation",
                "type": "global",
                "status": "current",
                "description": {
                    "plain": {
                        "value": "Documentation space"
                    }
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/wiki/api/v2/spaces?keys=DOCS").await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_create_space() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/wiki/api/v2/spaces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "999888",
            "key": "NEW",
            "name": "New Space",
            "type": "global",
            "status": "current"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "key": "NEW",
        "name": "New Space",
        "description": {"plain": {"value": "A new space"}}
    });

    let response: Result<serde_json::Value, _> = client.post("/wiki/api/v2/spaces", &payload).await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["key"], "NEW");
}

// ============================================================================
// Page Operations Tests
// ============================================================================

#[tokio::test]
async fn test_list_pages() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/pages"))
        .and(query_param("space-key", "DOCS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "id": "100001",
                    "title": "Getting Started",
                    "type": "page",
                    "status": "current"
                },
                {
                    "id": "100002",
                    "title": "API Reference",
                    "type": "page",
                    "status": "current"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/wiki/api/v2/pages?space-key=DOCS").await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["results"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_get_page() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/pages/100001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "100001",
            "title": "Getting Started",
            "type": "page",
            "status": "current",
            "version": {
                "number": 3,
                "message": "Updated content"
            },
            "body": {
                "storage": {
                    "value": "<p>Welcome to the documentation</p>",
                    "representation": "storage"
                }
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/wiki/api/v2/pages/100001").await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["title"], "Getting Started");
    assert_eq!(data["version"]["number"], 3);
}

#[tokio::test]
async fn test_create_page() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/wiki/api/v2/pages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "200001",
            "title": "New Page",
            "type": "page",
            "status": "current"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "spaceId": "123456",
        "title": "New Page",
        "status": "current",
        "body": {
            "representation": "storage",
            "value": "<p>Page content</p>"
        }
    });

    let response: Result<serde_json::Value, _> = client.post("/wiki/api/v2/pages", &payload).await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["id"], "200001");
}

#[tokio::test]
async fn test_update_page() {
    let mock_server = MockServer::start().await;

    // First mock: get current page version
    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/pages/100001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "100001",
            "title": "Original Title",
            "version": {"number": 2}
        })))
        .mount(&mock_server)
        .await;

    // Second mock: update page
    Mock::given(method("PUT"))
        .and(path("/wiki/api/v2/pages/100001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "100001",
            "title": "Updated Title",
            "version": {"number": 3}
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "id": "100001",
        "title": "Updated Title",
        "status": "current",
        "version": {"number": 3}
    });

    let response: Result<serde_json::Value, _> =
        client.put("/wiki/api/v2/pages/100001", &payload).await;

    assert!(response.is_ok());
}

// ============================================================================
// Blog Operations Tests
// ============================================================================

#[tokio::test]
async fn test_list_blogposts() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/blogposts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "id": "300001",
                    "title": "Weekly Update",
                    "status": "current"
                },
                {
                    "id": "300002",
                    "title": "Release Notes",
                    "status": "current"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/wiki/api/v2/blogposts").await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["results"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_create_blogpost() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/wiki/api/v2/blogposts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "300003",
            "title": "New Blog Post",
            "status": "current"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "spaceId": "123456",
        "title": "New Blog Post",
        "status": "current",
        "type": "blogpost",
        "body": {
            "representation": "storage",
            "value": "<p>Blog content</p>"
        }
    });

    let response: Result<serde_json::Value, _> =
        client.post("/wiki/api/v2/blogposts", &payload).await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["id"], "300003");
}

// ============================================================================
// Attachment Operations Tests
// ============================================================================

#[tokio::test]
async fn test_list_attachments() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/pages/100001/attachments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "id": "att-001",
                    "title": "diagram.png",
                    "fileSize": 102400,
                    "mediaType": "image/png"
                },
                {
                    "id": "att-002",
                    "title": "report.pdf",
                    "fileSize": 524288,
                    "mediaType": "application/pdf"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/wiki/api/v2/pages/100001/attachments").await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["results"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_get_attachment() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/attachments/att-001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "att-001",
            "title": "diagram.png",
            "fileSize": 102400,
            "mediaType": "image/png",
            "downloadLink": "/wiki/download/attachments/100001/diagram.png"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/wiki/api/v2/attachments/att-001").await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["id"], "att-001");
    assert_eq!(data["fileSize"], 102400);
}

// ============================================================================
// Search Operations Tests
// ============================================================================

#[tokio::test]
async fn test_search_cql() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/rest/api/content/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "content": {
                        "id": "100001",
                        "type": "page",
                        "title": "Search Result 1"
                    }
                },
                {
                    "content": {
                        "id": "100002",
                        "type": "page",
                        "title": "Search Result 2"
                    }
                }
            ],
            "size": 2
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/wiki/rest/api/content/search?cql=type%3Dpage")
        .await;

    assert!(response.is_ok());
    let data = response.unwrap();
    assert_eq!(data["size"], 2);
}

#[tokio::test]
async fn test_search_text() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/rest/api/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "content": {
                        "id": "100003",
                        "title": "Documentation Page"
                    }
                }
            ],
            "size": 1
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/wiki/rest/api/search?cql=text~%22documentation%22")
        .await;

    assert!(response.is_ok());
}

// ============================================================================
// Bulk Operations Tests
// ============================================================================

#[tokio::test]
async fn test_bulk_export_json() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/wiki/rest/api/content/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "content": {
                        "id": "100001"
                    }
                },
                {
                    "content": {
                        "id": "100002"
                    }
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/pages/100001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "100001",
            "title": "Page 1",
            "body": {"storage": {"value": "<p>Content 1</p>"}}
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/wiki/api/v2/pages/100002"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "100002",
            "title": "Page 2",
            "body": {"storage": {"value": "<p>Content 2</p>"}}
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    // Test search to get page IDs
    let response: Result<serde_json::Value, _> = client
        .get("/wiki/rest/api/content/search?cql=type%3Dpage")
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_bulk_add_labels() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/wiki/rest/api/content/100001/label"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {"prefix": "global", "name": "archived"}
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!([{
        "prefix": "global",
        "name": "archived"
    }]);

    let response: Result<serde_json::Value, _> = client
        .post("/wiki/rest/api/content/100001/label", &payload)
        .await;

    assert!(response.is_ok());
}
