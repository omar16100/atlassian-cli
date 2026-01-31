#![allow(dead_code)]

use anyhow::Context;
use atlassian_cli_api::ApiClient;
use atlassian_cli_output::OutputRenderer;
use serde::Deserialize;

/// Context for Bamboo operations.
pub struct BambooContext<'a> {
    pub client: ApiClient,
    pub renderer: &'a OutputRenderer,
}

impl BambooContext<'_> {
    pub async fn verify_auth(&self) -> anyhow::Result<()> {
        let _: serde_json::Value =
            self.client.get("/rest/api/latest/info").await.context(
                "Authentication may be expired or invalid. Run: atlassian-cli auth test",
            )?;
        Ok(())
    }
}

/// Project from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Plan from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct Plan {
    pub key: String,
    #[serde(rename = "shortKey")]
    pub short_key: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "projectKey")]
    pub project_key: Option<String>,
    #[serde(rename = "projectName")]
    pub project_name: Option<String>,
    pub enabled: Option<bool>,
    #[serde(rename = "isBuilding")]
    pub is_building: Option<bool>,
    #[serde(rename = "averageBuildTimeInSeconds")]
    pub average_build_time: Option<i64>,
}

/// Branch from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    pub key: String,
    #[serde(rename = "shortKey")]
    pub short_key: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

/// Build result from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct BuildResult {
    #[serde(rename = "buildResultKey")]
    pub build_result_key: String,
    #[serde(rename = "planKey")]
    pub plan_key: Option<PlanKey>,
    #[serde(rename = "buildNumber")]
    pub build_number: i64,
    #[serde(rename = "buildState")]
    pub build_state: Option<String>,
    pub state: Option<String>,
    #[serde(rename = "lifeCycleState")]
    pub life_cycle_state: Option<String>,
    #[serde(rename = "buildStartedTime")]
    pub build_started_time: Option<String>,
    #[serde(rename = "buildCompletedTime")]
    pub build_completed_time: Option<String>,
    #[serde(rename = "buildDurationInSeconds")]
    pub build_duration: Option<i64>,
    #[serde(rename = "reasonSummary")]
    pub reason_summary: Option<String>,
    #[serde(rename = "successfulTestCount")]
    pub successful_test_count: Option<i64>,
    #[serde(rename = "failedTestCount")]
    pub failed_test_count: Option<i64>,
}

/// Plan key wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanKey {
    pub key: String,
}

/// Agent from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct Agent {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub agent_type: Option<String>,
    pub active: Option<bool>,
    pub enabled: Option<bool>,
    pub busy: Option<bool>,
}

/// Deployment project from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentProject {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "planKey")]
    pub plan_key: Option<PlanKeyWrapper>,
}

/// Plan key wrapper for deployment.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanKeyWrapper {
    pub key: String,
}

/// Environment from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct Environment {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "deploymentProjectId")]
    pub deployment_project_id: Option<i64>,
}

/// Deployment result from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentResult {
    pub id: i64,
    #[serde(rename = "deploymentVersion")]
    pub deployment_version: Option<DeploymentVersion>,
    #[serde(rename = "deploymentVersionName")]
    pub deployment_version_name: Option<String>,
    #[serde(rename = "deploymentState")]
    pub deployment_state: Option<String>,
    #[serde(rename = "lifeCycleState")]
    pub life_cycle_state: Option<String>,
    #[serde(rename = "startedDate")]
    pub started_date: Option<i64>,
    #[serde(rename = "finishedDate")]
    pub finished_date: Option<i64>,
}

/// Deployment version.
#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentVersion {
    pub id: i64,
    pub name: String,
}

/// Server info from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub version: Option<String>,
    pub edition: Option<String>,
    #[serde(rename = "buildDate")]
    pub build_date: Option<String>,
    #[serde(rename = "buildNumber")]
    pub build_number: Option<String>,
    pub state: Option<String>,
}

/// Wrapper for list responses.
#[derive(Debug, Clone, Deserialize)]
pub struct ListResponse<T> {
    pub size: Option<i64>,
    #[serde(rename = "max-result")]
    pub max_result: Option<i64>,
    #[serde(rename = "start-index")]
    pub start_index: Option<i64>,
    #[serde(flatten)]
    pub items: T,
}

/// Projects wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectsWrapper {
    pub project: Option<Vec<Project>>,
}

/// Plans wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct PlansWrapper {
    pub plan: Option<Vec<Plan>>,
}

/// Branches wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct BranchesWrapper {
    pub branch: Option<Vec<Branch>>,
}

/// Results wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct ResultsWrapper {
    pub result: Option<Vec<BuildResult>>,
}

/// Results container for build history.
#[derive(Debug, Clone, Deserialize)]
pub struct ResultsContainer {
    pub results: ResultsWrapper,
}

/// Artifact from Bamboo API.
#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub link: Option<ArtifactLink>,
    #[serde(rename = "producerJobKey")]
    pub producer_job_key: Option<String>,
    pub shared: Option<bool>,
    pub size: Option<i64>,
}

/// Artifact link containing the download URL.
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactLink {
    pub href: String,
}

/// Artifacts container for API response.
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactsContainer {
    pub artifact: Option<Vec<Artifact>>,
}

/// Build with artifacts expanded.
#[derive(Debug, Clone, Deserialize)]
pub struct BuildWithArtifacts {
    #[serde(rename = "buildResultKey")]
    pub build_result_key: String,
    pub artifacts: Option<ArtifactsContainer>,
}
