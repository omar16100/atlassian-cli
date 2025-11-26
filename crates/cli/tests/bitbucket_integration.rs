use atlassian_cli_api::ApiClient;
use wiremock::matchers::{method, path, query_param, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_bitbucket_list_repos() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace"))
        .and(query_param("pagelen", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                {
                    "slug": "repo1",
                    "name": "Repository 1",
                    "is_private": true,
                    "mainbranch": {"name": "main"},
                    "language": "rust"
                },
                {
                    "slug": "repo2",
                    "name": "Repository 2",
                    "is_private": false,
                    "mainbranch": {"name": "master"},
                    "language": "python"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/2.0/repositories/myworkspace?pagelen=25").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bitbucket_get_repo() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace/myrepo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": "myrepo",
            "name": "My Repository",
            "full_name": "myworkspace/myrepo",
            "description": "Test repository",
            "is_private": true,
            "mainbranch": {"name": "main"},
            "language": "rust",
            "size": 102400
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/2.0/repositories/myworkspace/myrepo").await;

    assert!(response.is_ok());
    let repo = response.unwrap();
    assert_eq!(repo["slug"], "myrepo");
    assert_eq!(repo["language"], "rust");
}

#[tokio::test]
async fn test_bitbucket_create_repo() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/2.0/repositories/myworkspace/newrepo"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "slug": "newrepo",
            "name": "New Repository",
            "full_name": "myworkspace/newrepo",
            "is_private": true
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "scm": "git",
        "is_private": true,
        "name": "New Repository"
    });

    let response: Result<serde_json::Value, _> = client
        .post("/2.0/repositories/myworkspace/newrepo", &payload)
        .await;

    assert!(response.is_ok());
    let repo = response.unwrap();
    assert_eq!(repo["slug"], "newrepo");
}

#[tokio::test]
async fn test_bitbucket_update_repo() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/2.0/repositories/myworkspace/myrepo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "slug": "myrepo",
            "name": "Updated Name",
            "description": "Updated description",
            "language": "python"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "name": "Updated Name",
        "description": "Updated description"
    });

    let response: Result<serde_json::Value, _> = client
        .put("/2.0/repositories/myworkspace/myrepo", &payload)
        .await;

    assert!(response.is_ok());
    let repo = response.unwrap();
    assert_eq!(repo["name"], "Updated Name");
}

#[tokio::test]
async fn test_bitbucket_delete_repo() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/2.0/repositories/myworkspace/myrepo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.delete("/2.0/repositories/myworkspace/myrepo").await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_bitbucket_list_branches() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace/myrepo/refs/branches"))
        .and(query_param("pagelen", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                {
                    "name": "main",
                    "target": {
                        "hash": "abc123def456",
                        "author": {"raw": "John Doe <john@example.com>"},
                        "message": "Initial commit"
                    }
                },
                {
                    "name": "develop",
                    "target": {
                        "hash": "def456abc789",
                        "author": {"raw": "Jane Smith <jane@example.com>"},
                        "message": "Feature branch"
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
        .get("/2.0/repositories/myworkspace/myrepo/refs/branches?pagelen=25")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bitbucket_create_branch() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/2.0/repositories/myworkspace/myrepo/refs/branches"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "name": "feature/new-feature",
            "target": {
                "hash": "abc123def456"
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "name": "feature/new-feature",
        "target": {
            "hash": "abc123def456"
        }
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/refs/branches",
            &payload,
        )
        .await;

    assert!(response.is_ok());
    let branch = response.unwrap();
    assert_eq!(branch["name"], "feature/new-feature");
}

#[tokio::test]
async fn test_bitbucket_delete_branch() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/refs/branches/feature/old-feature",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .delete("/2.0/repositories/myworkspace/myrepo/refs/branches/feature/old-feature")
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_bitbucket_list_pull_requests() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace/myrepo/pullrequests"))
        .and(query_param("state", "OPEN"))
        .and(query_param("pagelen", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                {
                    "id": 1,
                    "title": "Add new feature",
                    "state": "OPEN",
                    "author": {"display_name": "John Doe"},
                    "source": {
                        "branch": {"name": "feature/new-feature"}
                    },
                    "destination": {
                        "branch": {"name": "main"}
                    }
                },
                {
                    "id": 2,
                    "title": "Fix bug",
                    "state": "OPEN",
                    "author": {"display_name": "Jane Smith"},
                    "source": {
                        "branch": {"name": "bugfix/issue-123"}
                    },
                    "destination": {
                        "branch": {"name": "develop"}
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
        .get("/2.0/repositories/myworkspace/myrepo/pullrequests?state=OPEN&pagelen=25")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bitbucket_create_pull_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/2.0/repositories/myworkspace/myrepo/pullrequests"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 3,
            "title": "New pull request",
            "state": "OPEN",
            "author": {"display_name": "Test User"},
            "source": {
                "branch": {"name": "feature/new"}
            },
            "destination": {
                "branch": {"name": "main"}
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "title": "New pull request",
        "source": {
            "branch": {"name": "feature/new"}
        },
        "destination": {
            "branch": {"name": "main"}
        }
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests",
            &payload,
        )
        .await;

    assert!(response.is_ok());
    let pr = response.unwrap();
    assert_eq!(pr["id"], 3);
    assert_eq!(pr["state"], "OPEN");
}

#[tokio::test]
async fn test_bitbucket_merge_pull_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/1/merge",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "title": "Add new feature",
            "state": "MERGED",
            "source": {
                "branch": {"name": "feature/new-feature"}
            },
            "destination": {
                "branch": {"name": "main"}
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({"merge_strategy": "merge_commit"});

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/1/merge",
            &payload,
        )
        .await;

    assert!(response.is_ok());
    let pr = response.unwrap();
    assert_eq!(pr["state"], "MERGED");
}

#[tokio::test]
async fn test_bitbucket_approve_pull_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/1/approve",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "approved": true,
            "user": {"display_name": "Test User"}
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/1/approve",
            &serde_json::json!({}),
        )
        .await;

    assert!(response.is_ok());
    let approval = response.unwrap();
    assert_eq!(approval["approved"], true);
}

#[tokio::test]
async fn test_bitbucket_branch_protection() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/branch-restrictions",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 123,
            "kind": "restrict_merges",
            "pattern": "main",
            "value": 2
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "kind": "restrict_merges",
        "pattern": "main",
        "value": 2
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/branch-restrictions",
            &payload,
        )
        .await;

    assert!(response.is_ok());
    let restriction = response.unwrap();
    assert_eq!(restriction["pattern"], "main");
    assert_eq!(restriction["value"], 2);
}

#[tokio::test]
async fn test_bitbucket_error_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace/notfound"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {
                "message": "Repository not found"
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/2.0/repositories/myworkspace/notfound").await;

    assert!(response.is_err());
}

// ============================================================================
// Pipeline Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_list_with_branch_filter() {
    let mock_server = MockServer::start().await;

    // Verify request uses q= filter syntax for branch filtering
    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace/myrepo/pipelines"))
        .and(query_param_contains("q", "target.ref_name"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                {
                    "uuid": "{123e4567-e89b-12d3-a456-426614174000}",
                    "build_number": 42,
                    "state": {"name": "COMPLETED", "result": {"name": "SUCCESSFUL"}},
                    "target": {"ref_name": "main", "type": "pipeline_ref_target"}
                }
            ],
            "next": null
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    // The actual request would come from list_pipelines, but we test the API layer
    let response: Result<serde_json::Value, _> = client
        .get("/2.0/repositories/myworkspace/myrepo/pipelines?q=target.ref_name%3D%22main%22&pagelen=100&sort=-created_on")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["values"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_pipeline_get_by_build_number_direct_filter() {
    let mock_server = MockServer::start().await;

    // Mock the direct q=build_number filter
    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace/myrepo/pipelines"))
        .and(query_param_contains("q", "build_number"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                {
                    "uuid": "{abc-def-123}",
                    "build_number": 404,
                    "state": {"name": "COMPLETED", "result": {"name": "SUCCESSFUL"}},
                    "target": {"ref_name": "main"}
                }
            ],
            "next": null
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/2.0/repositories/myworkspace/myrepo/pipelines?q=build_number%3D404&pagelen=1")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    let values = result["values"].as_array().unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["build_number"], 404);
    assert_eq!(values[0]["uuid"], "{abc-def-123}");
}

#[tokio::test]
async fn test_pipeline_list_pagination() {
    let mock_server = MockServer::start().await;

    // First page
    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace/myrepo/pipelines"))
        .and(query_param("pagelen", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                {"uuid": "{uuid-1}", "build_number": 100, "state": {"name": "COMPLETED"}}
            ],
            "next": "https://api.bitbucket.org/2.0/repositories/myworkspace/myrepo/pipelines?page=2"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/2.0/repositories/myworkspace/myrepo/pipelines?pagelen=100&sort=-created_on")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert!(result["next"].is_string()); // Verify pagination link exists
}
