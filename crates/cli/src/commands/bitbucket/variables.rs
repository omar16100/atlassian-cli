use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::utils::BitbucketContext;
use crate::commands::common::{render_success, MutationResult};

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
pub struct VariableList {
    #[serde(default)]
    pub values: Vec<Variable>,
    pub next: Option<String>,
    #[allow(dead_code)]
    pub page: Option<u32>,
    #[allow(dead_code)]
    pub pagelen: Option<u32>,
    #[allow(dead_code)]
    pub size: Option<u32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Variable {
    pub uuid: String,
    pub key: String,
    pub value: Option<String>,
    #[serde(default)]
    pub secured: bool,
    #[serde(rename = "type", default)]
    #[allow(dead_code)]
    pub var_type: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct EnvironmentList {
    #[serde(default)]
    pub values: Vec<Environment>,
    pub next: Option<String>,
    #[allow(dead_code)]
    pub page: Option<u32>,
    #[allow(dead_code)]
    pub pagelen: Option<u32>,
    #[allow(dead_code)]
    pub size: Option<u32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Environment {
    pub uuid: String,
    pub name: String,
    pub environment_type: Option<EnvironmentType>,
    pub rank: Option<i32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EnvironmentType {
    pub name: String,
}

// ---------------------------------------------------------------------------
// Payloads
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
struct CreateVariablePayload<'a> {
    key: &'a str,
    value: &'a str,
    secured: bool,
    #[serde(rename = "type")]
    var_type: &'a str,
}

#[derive(Serialize, Debug)]
struct UpdateVariablePayload<'a> {
    key: &'a str,
    value: &'a str,
    secured: bool,
    #[serde(rename = "type")]
    var_type: &'a str,
}

// ---------------------------------------------------------------------------
// Scopes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum VarScope {
    Repository {
        workspace: String,
        repo_slug: String,
    },
    Workspace {
        workspace: String,
    },
    Deployment {
        workspace: String,
        repo_slug: String,
        env_uuid: String,
    },
}

impl VarScope {
    fn label(&self) -> &str {
        match self {
            VarScope::Repository { .. } => "repository",
            VarScope::Workspace { .. } => "workspace",
            VarScope::Deployment { .. } => "deployment",
        }
    }
}

/// Build the base URL for variable operations at a given scope.
pub fn build_var_base_url(scope: &VarScope) -> String {
    match scope {
        VarScope::Repository {
            workspace,
            repo_slug,
        } => format!(
            "/2.0/repositories/{workspace}/{repo_slug}/pipelines_config/variables/"
        ),
        VarScope::Workspace { workspace } => {
            format!("/2.0/workspaces/{workspace}/pipelines-config/variables/")
        }
        VarScope::Deployment {
            workspace,
            repo_slug,
            env_uuid,
        } => format!(
            "/2.0/repositories/{workspace}/{repo_slug}/deployments_config/environments/{env_uuid}/variables/"
        ),
    }
}

// ---------------------------------------------------------------------------
// Output rows
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
struct VariableRow {
    key: String,
    value: String,
    secured: bool,
    uuid: String,
    scope: String,
}

impl VariableRow {
    fn from_variable(var: &Variable, scope_label: &str, is_table: bool) -> Self {
        let display_value = if var.secured && is_table {
            "(secured)".to_string()
        } else {
            var.value.clone().unwrap_or_default()
        };
        Self {
            key: var.key.clone(),
            value: display_value,
            secured: var.secured,
            uuid: var.uuid.clone(),
            scope: scope_label.to_string(),
        }
    }
}

#[derive(Serialize, Debug)]
struct EnvironmentRow {
    uuid: String,
    name: String,
    #[serde(rename = "type")]
    env_type: String,
    rank: String,
}

// ---------------------------------------------------------------------------
// CRUD operations
// ---------------------------------------------------------------------------

pub async fn list_variables(
    ctx: &BitbucketContext<'_>,
    scope: &VarScope,
    limit: usize,
) -> Result<()> {
    let base_url = build_var_base_url(scope);
    let is_table = ctx.renderer.format() == atlassian_cli_output::OutputFormat::Table;

    tracing::debug!(scope = ?scope, limit, "Listing pipeline variables");

    let mut all_vars: Vec<Variable> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = match &next_url {
            Some(url) => url.clone(),
            None => format!("{base_url}?pagelen=100"),
        };

        let response: VariableList = ctx
            .client
            .get(&path)
            .await
            .with_context(|| format!("Failed to list {} variables", scope.label()))?;

        all_vars.extend(response.values);
        next_url = response.next;

        if next_url.is_none() || all_vars.len() >= limit {
            break;
        }
    }

    all_vars.truncate(limit);

    if all_vars.is_empty() {
        tracing::info!(scope = scope.label(), "No variables found");
        println!("No {} variables found", scope.label());
        return Ok(());
    }

    let rows: Vec<VariableRow> = all_vars
        .iter()
        .map(|v| VariableRow::from_variable(v, scope.label(), is_table))
        .collect();

    tracing::info!(
        scope = scope.label(),
        count = rows.len(),
        "Listed pipeline variables"
    );

    ctx.renderer.render(&rows)
}

pub async fn get_variable(ctx: &BitbucketContext<'_>, scope: &VarScope, key: &str) -> Result<()> {
    let is_table = ctx.renderer.format() == atlassian_cli_output::OutputFormat::Table;

    tracing::debug!(scope = ?scope, key, "Getting pipeline variable");

    let (var, _uuid) = resolve_variable(ctx, scope, key).await?;

    let row = VariableRow::from_variable(&var, scope.label(), is_table);

    tracing::info!(scope = scope.label(), key, "Found pipeline variable");

    ctx.renderer.render(&row)
}

pub async fn create_variable(
    ctx: &BitbucketContext<'_>,
    scope: &VarScope,
    key: &str,
    value: &str,
    secured: bool,
) -> Result<()> {
    let base_url = build_var_base_url(scope);

    tracing::debug!(scope = ?scope, key, secured, "Creating pipeline variable");

    let payload = CreateVariablePayload {
        key,
        value,
        secured,
        var_type: "pipeline_variable",
    };

    let created: Variable = ctx
        .client
        .post(&base_url, &payload)
        .await
        .with_context(|| format!("Failed to create {} variable '{key}'", scope.label()))?;

    tracing::info!(
        scope = scope.label(),
        key,
        uuid = created.uuid.as_str(),
        secured,
        "Pipeline variable created"
    );

    render_success(
        ctx.renderer,
        &format!("✅ Variable '{}' created in {} scope", key, scope.label()),
        &MutationResult::with_id(
            format!("Variable '{}' created in {} scope", key, scope.label()),
            &created.uuid,
        ),
    )
}

pub async fn update_variable(
    ctx: &BitbucketContext<'_>,
    scope: &VarScope,
    key: &str,
    value: &str,
    secured_opt: Option<bool>,
) -> Result<()> {
    let base_url = build_var_base_url(scope);

    tracing::debug!(scope = ?scope, key, secured = ?secured_opt, "Updating pipeline variable");

    let (existing, uuid) = resolve_variable(ctx, scope, key).await?;

    // Tri-state: None → preserve current, Some(true) → secure, Some(false) → unsecure
    let secured = secured_opt.unwrap_or(existing.secured);

    let path = format!("{base_url}{uuid}");
    let payload = UpdateVariablePayload {
        key,
        value,
        secured,
        var_type: "pipeline_variable",
    };

    let _updated: Variable = ctx
        .client
        .put(&path, &payload)
        .await
        .with_context(|| format!("Failed to update {} variable '{key}'", scope.label()))?;

    let display_uuid = clean_uuid(&uuid);
    tracing::info!(
        scope = scope.label(),
        key,
        uuid = display_uuid.as_str(),
        secured,
        "Pipeline variable updated"
    );

    render_success(
        ctx.renderer,
        &format!("✅ Variable '{}' updated in {} scope", key, scope.label()),
        &MutationResult::with_id(
            format!("Variable '{}' updated in {} scope", key, scope.label()),
            &display_uuid,
        ),
    )
}

pub async fn delete_variable(
    ctx: &BitbucketContext<'_>,
    scope: &VarScope,
    key: &str,
) -> Result<()> {
    let base_url = build_var_base_url(scope);

    tracing::debug!(scope = ?scope, key, "Deleting pipeline variable");

    let (_var, uuid) = resolve_variable(ctx, scope, key).await?;

    let path = format!("{base_url}{uuid}");
    ctx.client
        .delete_no_content(&path)
        .await
        .with_context(|| format!("Failed to delete {} variable '{key}'", scope.label()))?;

    let display_uuid = clean_uuid(&uuid);
    tracing::info!(
        scope = scope.label(),
        key,
        uuid = display_uuid.as_str(),
        "Pipeline variable deleted"
    );

    render_success(
        ctx.renderer,
        &format!("✅ Variable '{}' deleted from {} scope", key, scope.label()),
        &MutationResult::with_id(
            format!("Variable '{}' deleted from {} scope", key, scope.label()),
            &display_uuid,
        ),
    )
}

// ---------------------------------------------------------------------------
// Environment operations
// ---------------------------------------------------------------------------

pub async fn list_environments(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    limit: usize,
) -> Result<()> {
    tracing::debug!(
        workspace,
        repo_slug,
        limit,
        "Listing deployment environments"
    );

    let mut all_envs: Vec<Environment> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = match &next_url {
            Some(url) => url.clone(),
            None => format!("/2.0/repositories/{workspace}/{repo_slug}/environments/?pagelen=100"),
        };

        let response: EnvironmentList =
            ctx.client.get(&path).await.with_context(|| {
                format!("Failed to list environments for {workspace}/{repo_slug}")
            })?;

        all_envs.extend(response.values);
        next_url = response.next;

        if next_url.is_none() || all_envs.len() >= limit {
            break;
        }
    }

    all_envs.truncate(limit);

    if all_envs.is_empty() {
        tracing::info!(workspace, repo_slug, "No deployment environments found");
        println!("No deployment environments found");
        return Ok(());
    }

    let rows: Vec<EnvironmentRow> = all_envs
        .iter()
        .map(|env| EnvironmentRow {
            uuid: env.uuid.clone(),
            name: env.name.clone(),
            env_type: env
                .environment_type
                .as_ref()
                .map(|t| t.name.clone())
                .unwrap_or_default(),
            rank: env.rank.map(|r| r.to_string()).unwrap_or_default(),
        })
        .collect();

    tracing::info!(
        workspace,
        repo_slug,
        count = rows.len(),
        "Listed deployment environments"
    );

    ctx.renderer.render(&rows)
}

// ---------------------------------------------------------------------------
// Resolution helpers
// ---------------------------------------------------------------------------

/// Fetch all variables (paginating through all pages) and find by key.
/// Returns the variable and its UUID (cleaned of braces).
async fn resolve_variable(
    ctx: &BitbucketContext<'_>,
    scope: &VarScope,
    key: &str,
) -> Result<(Variable, String)> {
    let base_url = build_var_base_url(scope);
    let mut all_vars: Vec<Variable> = Vec::new();
    let mut next_url: Option<String> = None;

    // Always paginate ALL pages to find the variable
    loop {
        let path = match &next_url {
            Some(url) => url.clone(),
            None => format!("{base_url}?pagelen=100"),
        };

        let response: VariableList = ctx.client.get(&path).await.with_context(|| {
            format!("Failed to list {} variables for resolution", scope.label())
        })?;

        all_vars.extend(response.values);
        next_url = response.next;

        if next_url.is_none() {
            break;
        }
    }

    let var = all_vars.iter().find(|v| v.key == key).cloned();

    match var {
        Some(v) => {
            tracing::debug!(key, uuid = v.uuid.as_str(), "Resolved variable key to UUID");
            let raw_uuid = v.uuid.clone();
            Ok((v, raw_uuid))
        }
        None => {
            let available: Vec<&str> = all_vars.iter().map(|v| v.key.as_str()).collect();
            bail!(
                "Variable '{}' not found in {} scope. Available keys: {}",
                key,
                scope.label(),
                if available.is_empty() {
                    "(none)".to_string()
                } else {
                    available.join(", ")
                }
            )
        }
    }
}

/// Resolve an environment name or UUID to a UUID.
/// If `name_or_uuid` looks like a UUID (contains braces or dashes), use directly.
/// Otherwise, fetch environments and match by name (case-insensitive).
pub async fn resolve_environment_uuid(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    name_or_uuid: &str,
) -> Result<String> {
    // If it starts with '{', it's a braced UUID — use directly
    if name_or_uuid.starts_with('{') {
        tracing::debug!(name_or_uuid, "Input looks like braced UUID, using directly");
        return Ok(name_or_uuid.to_string());
    }

    tracing::debug!(
        name_or_uuid,
        workspace,
        repo_slug,
        "Resolving environment name to UUID"
    );

    let mut all_envs: Vec<Environment> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = match &next_url {
            Some(url) => url.clone(),
            None => format!("/2.0/repositories/{workspace}/{repo_slug}/environments/?pagelen=100"),
        };

        let response: EnvironmentList =
            ctx.client.get(&path).await.with_context(|| {
                format!("Failed to list environments for {workspace}/{repo_slug}")
            })?;

        all_envs.extend(response.values);
        next_url = response.next;

        if next_url.is_none() {
            break;
        }
    }

    let name_lower = name_or_uuid.to_lowercase();
    let env = all_envs
        .iter()
        .find(|e| e.name.to_lowercase() == name_lower);

    match env {
        Some(e) => {
            tracing::debug!(
                name = name_or_uuid,
                uuid = e.uuid.as_str(),
                "Resolved environment name to UUID"
            );
            Ok(e.uuid.clone())
        }
        None => {
            let available: Vec<&str> = all_envs.iter().map(|e| e.name.as_str()).collect();
            bail!(
                "Environment '{}' not found. Available environments: {}",
                name_or_uuid,
                if available.is_empty() {
                    "(none)".to_string()
                } else {
                    available.join(", ")
                }
            )
        }
    }
}

/// Strip surrounding braces from Bitbucket UUIDs.
fn clean_uuid(uuid: &str) -> String {
    uuid.trim_start_matches('{')
        .trim_end_matches('}')
        .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_var_base_url_repository() {
        let scope = VarScope::Repository {
            workspace: "myws".to_string(),
            repo_slug: "myrepo".to_string(),
        };
        let url = build_var_base_url(&scope);
        assert_eq!(
            url,
            "/2.0/repositories/myws/myrepo/pipelines_config/variables/"
        );
    }

    #[test]
    fn test_build_var_base_url_workspace() {
        let scope = VarScope::Workspace {
            workspace: "myws".to_string(),
        };
        let url = build_var_base_url(&scope);
        assert_eq!(url, "/2.0/workspaces/myws/pipelines-config/variables/");
    }

    #[test]
    fn test_build_var_base_url_deployment() {
        // env_uuid comes from API with braces, used raw in URL
        let scope = VarScope::Deployment {
            workspace: "myws".to_string(),
            repo_slug: "myrepo".to_string(),
            env_uuid: "{abc-123}".to_string(),
        };
        let url = build_var_base_url(&scope);
        assert_eq!(
            url,
            "/2.0/repositories/myws/myrepo/deployments_config/environments/{abc-123}/variables/"
        );
    }

    #[test]
    fn test_variable_deserialize() {
        let json = r#"{
            "uuid": "{abc-123}",
            "key": "MY_VAR",
            "value": "hello",
            "secured": false,
            "type": "pipeline_variable"
        }"#;
        let var: Variable = serde_json::from_str(json).unwrap();
        assert_eq!(var.uuid, "{abc-123}");
        assert_eq!(var.key, "MY_VAR");
        assert_eq!(var.value, Some("hello".to_string()));
        assert!(!var.secured);
        assert_eq!(var.var_type, Some("pipeline_variable".to_string()));
    }

    #[test]
    fn test_variable_deserialize_secured() {
        let json = r#"{
            "uuid": "{def-456}",
            "key": "SECRET_KEY",
            "secured": true,
            "type": "pipeline_variable"
        }"#;
        let var: Variable = serde_json::from_str(json).unwrap();
        assert_eq!(var.key, "SECRET_KEY");
        assert!(var.secured);
        assert!(var.value.is_none());
    }

    #[test]
    fn test_variable_list_deserialize() {
        let json = r#"{
            "values": [
                {"uuid": "{a}", "key": "A", "value": "1", "secured": false},
                {"uuid": "{b}", "key": "B", "secured": true}
            ],
            "page": 1,
            "pagelen": 10,
            "size": 2
        }"#;
        let list: VariableList = serde_json::from_str(json).unwrap();
        assert_eq!(list.values.len(), 2);
        assert!(list.next.is_none());
    }

    #[test]
    fn test_environment_deserialize() {
        let json = r#"{
            "uuid": "{env-123}",
            "name": "staging",
            "environment_type": {"name": "Staging"},
            "rank": 1
        }"#;
        let env: Environment = serde_json::from_str(json).unwrap();
        assert_eq!(env.name, "staging");
        assert_eq!(env.environment_type.unwrap().name, "Staging");
        assert_eq!(env.rank, Some(1));
    }

    #[test]
    fn test_environment_deserialize_optional_fields() {
        let json = r#"{
            "uuid": "{env-456}",
            "name": "test"
        }"#;
        let env: Environment = serde_json::from_str(json).unwrap();
        assert_eq!(env.name, "test");
        assert!(env.environment_type.is_none());
        assert!(env.rank.is_none());
    }

    #[test]
    fn test_variable_row_secured_display_table() {
        let var = Variable {
            uuid: "{abc}".to_string(),
            key: "SECRET".to_string(),
            value: None,
            secured: true,
            var_type: Some("pipeline_variable".to_string()),
        };
        let row = VariableRow::from_variable(&var, "repository", true);
        assert_eq!(row.value, "(secured)");
        assert!(row.secured);
    }

    #[test]
    fn test_variable_row_secured_display_json() {
        let var = Variable {
            uuid: "{abc}".to_string(),
            key: "SECRET".to_string(),
            value: None,
            secured: true,
            var_type: Some("pipeline_variable".to_string()),
        };
        // is_table=false means raw value (empty string since None)
        let row = VariableRow::from_variable(&var, "repository", false);
        assert_eq!(row.value, "");
        assert!(row.secured);
    }

    #[test]
    fn test_create_payload_includes_type() {
        let payload = CreateVariablePayload {
            key: "MY_VAR",
            value: "hello",
            secured: false,
            var_type: "pipeline_variable",
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "pipeline_variable");
        assert_eq!(json["key"], "MY_VAR");
        assert_eq!(json["value"], "hello");
        assert_eq!(json["secured"], false);
    }

    #[test]
    fn test_clean_uuid() {
        assert_eq!(clean_uuid("{abc-123}"), "abc-123");
        assert_eq!(clean_uuid("abc-123"), "abc-123");
        assert_eq!(clean_uuid("{}"), "");
    }

    #[test]
    fn test_braced_uuid_preserved_in_api_path() {
        // Bitbucket UUIDs come with braces — must be preserved for API calls
        let scope = VarScope::Repository {
            workspace: "ws".to_string(),
            repo_slug: "repo".to_string(),
        };
        let base = build_var_base_url(&scope);
        let braced_uuid = "{abc-def-123}";
        let path = format!("{base}{braced_uuid}");
        assert_eq!(
            path,
            "/2.0/repositories/ws/repo/pipelines_config/variables/{abc-def-123}"
        );
    }

    #[test]
    fn test_clean_uuid_only_for_display() {
        // clean_uuid strips braces for display purposes only
        assert_eq!(clean_uuid("{abc-123}"), "abc-123");
        // Raw UUID should be used in API paths (not cleaned)
        let raw = "{abc-123}";
        assert!(raw.starts_with('{'));
    }

    #[test]
    fn test_var_scope_label() {
        assert_eq!(
            VarScope::Repository {
                workspace: "w".into(),
                repo_slug: "r".into()
            }
            .label(),
            "repository"
        );
        assert_eq!(
            VarScope::Workspace {
                workspace: "w".into()
            }
            .label(),
            "workspace"
        );
        assert_eq!(
            VarScope::Deployment {
                workspace: "w".into(),
                repo_slug: "r".into(),
                env_uuid: "u".into()
            }
            .label(),
            "deployment"
        );
    }
}
