use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{ApiResponse, OpsgenieContext, PagedResponse, Team, TeamMember};

/// List all teams.
pub async fn list_teams(ctx: &OpsgenieContext<'_>, limit: usize) -> Result<()> {
    let path = format!("teams?limit={}", limit.min(100));
    tracing::debug!("Listing teams with limit {}", limit);

    let response: PagedResponse<Team> = ctx
        .client
        .get(&path)
        .await
        .context("Failed to list teams")?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .iter()
        .map(|team| Row {
            id: &team.id,
            name: &team.name,
            description: team.description.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No teams found");
    }

    tracing::info!("Found {} teams", rows.len());
    ctx.renderer.render_list_or_empty(&rows, "No teams found")
}

/// Get team details.
pub async fn get_team(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("teams/{}", identifier);
    tracing::debug!("Fetching team {}", identifier);

    let response: ApiResponse<Team> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch team {}", identifier))?;

    let team = response.data;

    #[derive(Serialize)]
    struct View<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
    }

    let view = View {
        id: &team.id,
        name: &team.name,
        description: team.description.as_deref().unwrap_or(""),
    };

    tracing::info!("Retrieved team {}", identifier);
    ctx.renderer.render(&view)
}

/// Create a new team.
pub async fn create_team(
    ctx: &OpsgenieContext<'_>,
    name: String,
    description: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct CreateTeamBody {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    }

    let body = CreateTeamBody {
        name: name.clone(),
        description,
    };

    tracing::debug!("Creating team: {}", name);

    #[derive(serde::Deserialize)]
    struct CreateResult {
        id: String,
        name: String,
    }

    let response: ApiResponse<CreateResult> = ctx
        .client
        .post("teams", &body)
        .await
        .context("Failed to create team")?;

    tracing::info!(
        "Created team {} (id: {})",
        response.data.name,
        response.data.id
    );
    println!(
        "Created team: {} (id: {})",
        response.data.name, response.data.id
    );
    Ok(())
}

/// Delete a team.
pub async fn delete_team(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    let path = format!("teams/{}", identifier);

    tracing::debug!("Deleting team {}", identifier);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to delete team {}", identifier))?;

    tracing::info!("Deleted team {}", identifier);
    println!("Successfully deleted team {}", identifier);
    Ok(())
}

/// List team members.
pub async fn list_members(ctx: &OpsgenieContext<'_>, identifier: &str) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct MembersData {
        members: Vec<TeamMember>,
    }

    let path = format!("teams/{}", identifier);
    tracing::debug!("Listing members for team {}", identifier);

    let response: ApiResponse<MembersData> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to list members for team {}", identifier))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        username: &'a str,
        role: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .members
        .iter()
        .map(|member| Row {
            id: member.user.id.as_deref().unwrap_or(""),
            username: member.user.username.as_deref().unwrap_or(""),
            role: member.role.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No members in team {}", identifier);
    }

    tracing::info!("Found {} members in team {}", rows.len(), identifier);
    ctx.renderer.render_list_or_empty(&rows, "No members found")
}

/// Add member to team.
pub async fn add_member(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    user_id: &str,
    role: Option<&str>,
) -> Result<()> {
    #[derive(Serialize)]
    struct AddMemberBody<'a> {
        user: UserRef<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        role: Option<&'a str>,
    }

    #[derive(Serialize)]
    struct UserRef<'a> {
        id: &'a str,
    }

    let path = format!("teams/{}/members", identifier);
    let body = AddMemberBody {
        user: UserRef { id: user_id },
        role,
    };

    tracing::debug!("Adding user {} to team {}", user_id, identifier);
    ctx.client
        .post::<serde_json::Value, _>(&path, &body)
        .await
        .with_context(|| format!("Failed to add user {} to team {}", user_id, identifier))?;

    tracing::info!("Added user {} to team {}", user_id, identifier);
    println!("Successfully added user {} to team {}", user_id, identifier);
    Ok(())
}

/// Remove member from team.
pub async fn remove_member(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    user_id: &str,
) -> Result<()> {
    let path = format!("teams/{}/members/{}", identifier, user_id);

    tracing::debug!("Removing user {} from team {}", user_id, identifier);
    ctx.client
        .delete::<serde_json::Value>(&path)
        .await
        .with_context(|| format!("Failed to remove user {} from team {}", user_id, identifier))?;

    tracing::info!("Removed user {} from team {}", user_id, identifier);
    println!(
        "Successfully removed user {} from team {}",
        user_id, identifier
    );
    Ok(())
}

/// Get team's on-call participants.
pub async fn get_on_call(
    ctx: &OpsgenieContext<'_>,
    identifier: &str,
    date: Option<&str>,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct OnCallData {
        #[serde(rename = "onCallParticipants")]
        on_call_participants: Vec<OnCallEntry>,
    }

    #[derive(serde::Deserialize)]
    struct OnCallEntry {
        id: Option<String>,
        name: Option<String>,
        #[serde(rename = "type")]
        participant_type: Option<String>,
    }

    let mut path = format!("teams/{}/on-calls", identifier);
    if let Some(d) = date {
        path = format!("{}?date={}", path, d);
    }

    tracing::debug!("Getting on-call for team {}", identifier);

    let response: ApiResponse<OnCallData> = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to get on-call for team {}", identifier))?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: &'a str,
        name: &'a str,
        participant_type: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .data
        .on_call_participants
        .iter()
        .map(|p| Row {
            id: p.id.as_deref().unwrap_or(""),
            name: p.name.as_deref().unwrap_or(""),
            participant_type: p.participant_type.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!("No one is on-call for team {}", identifier);
    }

    tracing::info!(
        "Found {} on-call participants for team {}",
        rows.len(),
        identifier
    );
    ctx.renderer
        .render_list_or_empty(&rows, "No one is on-call")
}
