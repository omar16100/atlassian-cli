use atlassian_cli_api::ApiClient;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_bamboo_list_projects() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "projects": {
                "size": 2,
                "project": [
                    {
                        "key": "PROJ1",
                        "name": "Project One",
                        "description": "First project"
                    },
                    {
                        "key": "PROJ2",
                        "name": "Project Two",
                        "description": "Second project"
                    }
                ]
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/latest/project").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["projects"]["project"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bamboo_get_project() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/project/PROJ1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "PROJ1",
            "name": "Project One",
            "description": "First project"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/latest/project/PROJ1").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["key"], "PROJ1");
}

#[tokio::test]
async fn test_bamboo_list_plans() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/plan"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "plans": {
                "size": 2,
                "plan": [
                    {
                        "key": "PROJ1-BUILD",
                        "shortKey": "BUILD",
                        "name": "Build Plan",
                        "enabled": true
                    },
                    {
                        "key": "PROJ1-DEPLOY",
                        "shortKey": "DEPLOY",
                        "name": "Deploy Plan",
                        "enabled": true
                    }
                ]
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/latest/plan").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["plans"]["plan"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bamboo_get_plan() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/plan/PROJ1-BUILD"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "PROJ1-BUILD",
            "shortKey": "BUILD",
            "name": "Build Plan",
            "description": "Main build plan",
            "enabled": true,
            "isBuilding": false,
            "averageBuildTimeInSeconds": 120
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/api/latest/plan/PROJ1-BUILD").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["key"], "PROJ1-BUILD");
}

#[tokio::test]
async fn test_bamboo_enable_plan() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/latest/plan/PROJ1-BUILD/enable"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .post(
            "/rest/api/latest/plan/PROJ1-BUILD/enable",
            &serde_json::json!({}),
        )
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_bamboo_list_branches() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/plan/PROJ1-BUILD/branch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "branches": {
                "size": 2,
                "branch": [
                    {
                        "key": "PROJ1-BUILD0",
                        "name": "main",
                        "enabled": true
                    },
                    {
                        "key": "PROJ1-BUILD1",
                        "name": "develop",
                        "enabled": true
                    }
                ]
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/api/latest/plan/PROJ1-BUILD/branch").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["branches"]["branch"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bamboo_create_branch() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/rest/api/latest/plan/PROJ1-BUILD/branch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "PROJ1-BUILD2"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "branchName": "feature-branch",
        "vcsBranch": "feature/new-feature"
    });

    let response: Result<serde_json::Value, _> = client
        .put("/rest/api/latest/plan/PROJ1-BUILD/branch", &payload)
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["key"], "PROJ1-BUILD2");
}

#[tokio::test]
async fn test_bamboo_list_builds() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/result/PROJ1-BUILD"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": {
                "size": 2,
                "result": [
                    {
                        "key": "PROJ1-BUILD-100",
                        "buildNumber": 100,
                        "buildState": "Successful",
                        "lifeCycleState": "Finished"
                    },
                    {
                        "key": "PROJ1-BUILD-99",
                        "buildNumber": 99,
                        "buildState": "Failed",
                        "lifeCycleState": "Finished"
                    }
                ]
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/api/latest/result/PROJ1-BUILD").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["results"]["result"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bamboo_get_build() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/result/PROJ1-BUILD-100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "key": "PROJ1-BUILD-100",
            "buildNumber": 100,
            "buildState": "Successful",
            "lifeCycleState": "Finished",
            "buildDuration": 120000,
            "buildStartedTime": "2025-01-15T10:00:00.000Z",
            "buildCompletedTime": "2025-01-15T10:02:00.000Z"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/api/latest/result/PROJ1-BUILD-100").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["buildState"], "Successful");
}

#[tokio::test]
async fn test_bamboo_run_build() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/latest/queue/PROJ1-BUILD"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "buildResultKey": "PROJ1-BUILD-101",
            "buildNumber": 101,
            "triggerReason": "Manual build"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .post("/rest/api/latest/queue/PROJ1-BUILD", &serde_json::json!({}))
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["buildResultKey"], "PROJ1-BUILD-101");
}

#[tokio::test]
async fn test_bamboo_stop_build() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/rest/api/latest/queue/PROJ1-BUILD-101"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .delete("/rest/api/latest/queue/PROJ1-BUILD-101")
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_bamboo_add_comment() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/latest/result/PROJ1-BUILD-100/comment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "content": "Build looks good!"
    });

    let response: Result<serde_json::Value, _> = client
        .post("/rest/api/latest/result/PROJ1-BUILD-100/comment", &payload)
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_bamboo_list_deployment_projects() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/deploy/project/all"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": 1,
                "name": "Production Deploy",
                "planKey": {"key": "PROJ1-BUILD"},
                "description": "Deploy to production"
            },
            {
                "id": 2,
                "name": "Staging Deploy",
                "planKey": {"key": "PROJ1-BUILD"},
                "description": "Deploy to staging"
            }
        ])))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/api/latest/deploy/project/all").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bamboo_get_deployment_project() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/deploy/project/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "name": "Production Deploy",
            "planKey": {"key": "PROJ1-BUILD"},
            "description": "Deploy to production",
            "environments": [
                {
                    "id": 1,
                    "name": "Production",
                    "description": "Production environment"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> =
        client.get("/rest/api/latest/deploy/project/1").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["name"], "Production Deploy");
}

#[tokio::test]
async fn test_bamboo_trigger_deployment() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/latest/queue/deployment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deploymentResultId": 12345
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "environmentId": 1,
        "versionId": 100
    });

    let response: Result<serde_json::Value, _> = client
        .post("/rest/api/latest/queue/deployment", &payload)
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["deploymentResultId"], 12345);
}

#[tokio::test]
async fn test_bamboo_list_agents() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/agent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": 1,
                "name": "Agent 1",
                "type": "LOCAL",
                "active": true,
                "enabled": true,
                "busy": false
            },
            {
                "id": 2,
                "name": "Agent 2",
                "type": "REMOTE",
                "active": true,
                "enabled": true,
                "busy": true
            }
        ])))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/latest/agent").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_bamboo_get_agent() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/agent/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "name": "Agent 1",
            "type": "LOCAL",
            "active": true,
            "enabled": true,
            "busy": false
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/latest/agent/1").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["name"], "Agent 1");
}

#[tokio::test]
async fn test_bamboo_enable_agent() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/rest/api/latest/agent/1/enable"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .put("/rest/api/latest/agent/1/enable", &serde_json::json!({}))
        .await;

    assert!(response.is_ok());
}

#[tokio::test]
async fn test_bamboo_list_queue() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/queue"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "queuedBuilds": {
                "size": 2,
                "queuedBuild": [
                    {
                        "buildResultKey": "PROJ1-BUILD-102",
                        "planKey": "PROJ1-BUILD",
                        "buildNumber": 102
                    },
                    {
                        "buildResultKey": "PROJ2-BUILD-50",
                        "planKey": "PROJ2-BUILD",
                        "buildNumber": 50
                    }
                ]
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/latest/queue").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(
        result["queuedBuilds"]["queuedBuild"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn test_bamboo_server_info() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version": "9.2.3",
            "edition": "Standard",
            "buildNumber": "92003",
            "buildDate": "2025-01-01",
            "state": "RUNNING"
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client.get("/rest/api/latest/info").await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["state"], "RUNNING");
}

#[tokio::test]
async fn test_bamboo_list_artifacts() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/latest/result/PROJ1-BUILD-101"))
        .and(query_param("expand", "artifacts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "buildResultKey": "PROJ1-BUILD-101",
            "artifacts": {
                "artifact": [
                    {
                        "name": "build-output.zip",
                        "link": {
                            "href": "/artifact/PROJ1-BUILD-101/build-output.zip"
                        },
                        "producerJobKey": "PROJ1-BUILD-JOB1",
                        "shared": false,
                        "size": 1048576
                    },
                    {
                        "name": "test-reports.html",
                        "link": {
                            "href": "/artifact/PROJ1-BUILD-101/test-reports.html"
                        },
                        "producerJobKey": "PROJ1-BUILD-JOB2",
                        "shared": true,
                        "size": 2048
                    }
                ]
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/rest/api/latest/result/PROJ1-BUILD-101?expand=artifacts")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    assert_eq!(result["artifacts"]["artifact"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["artifacts"]["artifact"][0]["name"],
        "build-output.zip"
    );
    assert_eq!(result["artifacts"]["artifact"][0]["size"], 1048576);
}

#[tokio::test]
async fn test_bamboo_get_bytes() {
    let mock_server = MockServer::start().await;

    let artifact_content = b"This is artifact content for testing";

    Mock::given(method("GET"))
        .and(path("/artifact/PROJ1-BUILD-101/build-output.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(artifact_content.to_vec()))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response = client
        .get_bytes("/artifact/PROJ1-BUILD-101/build-output.zip")
        .await;

    assert!(response.is_ok());
    let bytes = response.unwrap();
    assert_eq!(bytes, artifact_content);
}
