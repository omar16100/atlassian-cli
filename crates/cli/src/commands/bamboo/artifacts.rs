use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use super::utils::{BambooContext, BuildWithArtifacts};

/// List artifacts for a build.
pub async fn list_artifacts(ctx: &BambooContext<'_>, build_key: &str) -> Result<()> {
    debug!("Listing artifacts for build: {}", build_key);

    let path = format!("/rest/api/latest/result/{}?expand=artifacts", build_key);
    let response: BuildWithArtifacts = ctx
        .client
        .get(&path)
        .await
        .context("Failed to fetch build artifacts")?;

    let artifacts = response
        .artifacts
        .and_then(|a| a.artifact)
        .unwrap_or_default();

    info!(
        "Found {} artifacts for build {}",
        artifacts.len(),
        build_key
    );

    #[derive(Serialize)]
    struct Row<'a> {
        name: &'a str,
        size: String,
        shared: bool,
        job: &'a str,
    }

    let rows: Vec<Row<'_>> = artifacts
        .iter()
        .map(|a| Row {
            name: &a.name,
            size: a.size.map(format_size).unwrap_or_default(),
            shared: a.shared.unwrap_or(false),
            job: a.producer_job_key.as_deref().unwrap_or("-"),
        })
        .collect();

    ctx.renderer.render(&rows)
}

/// Download an artifact by name.
pub async fn download_artifact(
    ctx: &BambooContext<'_>,
    build_key: &str,
    artifact_name: &str,
    output_path: &Path,
) -> Result<()> {
    debug!(
        "Downloading artifact '{}' from build {}",
        artifact_name, build_key
    );

    let path = format!("/rest/api/latest/result/{}?expand=artifacts", build_key);
    let response: BuildWithArtifacts = ctx
        .client
        .get(&path)
        .await
        .context("Failed to fetch build artifacts")?;

    let artifacts = response
        .artifacts
        .and_then(|a| a.artifact)
        .unwrap_or_default();

    let artifact = artifacts
        .iter()
        .find(|a| a.name == artifact_name)
        .with_context(|| format!("Artifact '{}' not found", artifact_name))?;

    let download_url = artifact
        .link
        .as_ref()
        .map(|l| &l.href)
        .context("Artifact has no download URL")?;

    info!("Downloading from: {}", download_url);

    let bytes = ctx
        .client
        .get_bytes(download_url)
        .await
        .context("Failed to download artifact")?;

    let mut file = File::create(output_path)
        .await
        .context("Failed to create output file")?;
    file.write_all(&bytes)
        .await
        .context("Failed to write artifact to file")?;

    info!(
        "Downloaded {} bytes to {}",
        bytes.len(),
        output_path.display()
    );
    println!("Downloaded to {}", output_path.display());

    Ok(())
}

fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
