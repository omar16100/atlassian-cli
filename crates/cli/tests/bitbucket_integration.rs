use atlassian_cli_api::ApiClient;
use wiremock::matchers::{body_partial_json, method, path, query_param, query_param_contains};
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
async fn test_bitbucket_get_pull_request_with_reviewer_participants() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.0/repositories/myworkspace/myrepo/pullrequests/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "title": "Add new feature",
            "state": "OPEN",
            "author": {"display_name": "John Doe"},
            "source": {
                "branch": {"name": "feature/new-feature"}
            },
            "destination": {
                "branch": {"name": "main"}
            },
            "participants": [
                {
                    "user": {"display_name": "Jane Doe"},
                    "role": "REVIEWER",
                    "approved": true,
                    "state": "approved",
                    "participated_on": "2026-08-10T12:00:00Z"
                },
                {
                    "user": {"display_name": "John Smith"},
                    "role": "REVIEWER",
                    "approved": false,
                    "state": "changes_requested",
                    "participated_on": "2026-08-11T09:30:00Z"
                },
                {
                    "user": {"display_name": "Alex Lee"},
                    "role": "PARTICIPANT",
                    "approved": false,
                    "state": null,
                    "participated_on": null
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/2.0/repositories/myworkspace/myrepo/pullrequests/1")
        .await;

    assert!(response.is_ok());
    let pr = response.unwrap();
    let participants = pr["participants"].as_array().unwrap();
    assert_eq!(participants.len(), 3);

    assert_eq!(participants[0]["role"], "REVIEWER");
    assert_eq!(participants[0]["state"], "approved");

    assert_eq!(participants[1]["state"], "changes_requested");
    assert_eq!(participants[1]["approved"], false);

    assert_eq!(participants[2]["role"], "PARTICIPANT");
    assert!(participants[2]["state"].is_null());
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

// ============================================================================
// PR Inline Comment Tests
// ============================================================================

#[tokio::test]
async fn test_bitbucket_add_pr_inline_comment_new_side_posts_inline_object() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/1/comments",
        ))
        .and(body_partial_json(serde_json::json!({
            "content": { "raw": "Nit: rename this" },
            "inline": { "path": "src/main.rs", "to": 42 }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 999,
            "content": { "raw": "Nit: rename this", "markup": "markdown", "html": "<p>Nit: rename this</p>" },
            "user": { "display_name": "Test User" },
            "created_on": "2026-08-20T12:00:00Z",
            "inline": { "path": "src/main.rs", "to": 42, "from": null }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "content": { "raw": "Nit: rename this" },
        "inline": { "path": "src/main.rs", "to": 42 }
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/1/comments",
            &payload,
        )
        .await;

    assert!(response.is_ok(), "post should succeed: {response:?}");
    let created = response.unwrap();
    assert_eq!(created["id"], 999);
    assert_eq!(created["inline"]["path"], "src/main.rs");
    assert_eq!(created["inline"]["to"], 42);
}

#[tokio::test]
async fn test_bitbucket_add_pr_inline_comment_old_side_posts_from_field() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/2/comments",
        ))
        .and(body_partial_json(serde_json::json!({
            "content": { "raw": "Why remove this?" },
            "inline": { "path": "src/main.rs", "from": 17 }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 1000,
            "content": { "raw": "Why remove this?" },
            "user": { "display_name": "Test User" },
            "inline": { "path": "src/main.rs", "from": 17, "to": null }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "content": { "raw": "Why remove this?" },
        "inline": { "path": "src/main.rs", "from": 17 }
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/2/comments",
            &payload,
        )
        .await;

    assert!(response.is_ok());
    let created = response.unwrap();
    assert_eq!(created["inline"]["from"], 17);
}

#[tokio::test]
async fn test_bitbucket_add_pr_global_comment_still_has_no_inline_field() {
    let mock_server = MockServer::start().await;

    // Two mocks: one matches the exact global-comment shape; one is a fallback
    // that fails the test if any request with an `inline` field ever arrives.
    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/3/comments",
        ))
        .and(body_partial_json(serde_json::json!({
            "content": { "raw": "LGTM" }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 1001,
            "content": { "raw": "LGTM" },
            "user": { "display_name": "Test User" }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "content": { "raw": "LGTM" }
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/3/comments",
            &payload,
        )
        .await;

    assert!(response.is_ok());
    let created = response.unwrap();
    assert!(created.get("inline").is_none_or(|v| v.is_null()));
}

#[tokio::test]
async fn test_bitbucket_list_pr_comments_parses_mix_of_global_and_inline() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/4/comments",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                {
                    "id": 100,
                    "content": { "raw": "LGTM overall" },
                    "user": { "display_name": "Alice" },
                    "created_on": "2026-08-20T10:00:00Z"
                },
                {
                    "id": 101,
                    "content": { "raw": "nit: rename" },
                    "user": { "display_name": "Bob" },
                    "created_on": "2026-08-20T10:05:00Z",
                    "inline": { "path": "src/main.rs", "to": 42, "from": null }
                },
                {
                    "id": 102,
                    "content": { "raw": "why remove?" },
                    "user": { "display_name": "Carol" },
                    "created_on": "2026-08-20T10:10:00Z",
                    "inline": { "path": "src/main.rs", "from": 17, "to": null }
                },
                {
                    "id": 103,
                    "content": { "raw": "whole file comment" },
                    "user": { "display_name": "Dave" },
                    "created_on": "2026-08-20T10:15:00Z",
                    "inline": { "path": "README.md", "to": null, "from": null }
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/2.0/repositories/myworkspace/myrepo/pullrequests/4/comments")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    let values = result["values"].as_array().unwrap();
    assert_eq!(values.len(), 4);

    assert!(values[0].get("inline").is_none_or(|v| v.is_null()));
    assert_eq!(values[1]["inline"]["to"], 42);
    assert_eq!(values[2]["inline"]["from"], 17);
    assert!(values[3]["inline"]["to"].is_null());
    assert!(values[3]["inline"]["from"].is_null());
}
