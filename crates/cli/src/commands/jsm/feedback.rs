use anyhow::{Context, Result};
use serde::Serialize;

use super::utils::{Feedback, JsmContext};

/// Get feedback for a request.
pub async fn get_feedback(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/servicedeskapi/request/{key}/feedback");
    tracing::debug!("Fetching feedback for request {}", key);

    let feedback: Feedback = ctx
        .client
        .get(&path)
        .await
        .with_context(|| format!("Failed to fetch feedback for request {key}"))?;

    #[derive(Serialize)]
    struct View<'a> {
        rating: i32,
        comment: &'a str,
    }

    let view = View {
        rating: feedback.rating.unwrap_or(0),
        comment: feedback
            .comment
            .as_ref()
            .map(|c| c.body.as_str())
            .unwrap_or(""),
    };

    tracing::info!("Retrieved feedback for request {}", key);
    ctx.renderer.render(&view)
}

/// Submit CSAT feedback for a request.
pub async fn submit_feedback(
    ctx: &JsmContext<'_>,
    key: &str,
    rating: i32,
    comment: Option<String>,
) -> Result<()> {
    #[derive(Serialize)]
    struct FeedbackBody {
        rating: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        comment: Option<CommentBody>,
    }

    #[derive(Serialize)]
    struct CommentBody {
        body: String,
    }

    let path = format!("/rest/servicedeskapi/request/{key}/feedback");
    let body = FeedbackBody {
        rating,
        comment: comment.map(|c| CommentBody { body: c }),
    };

    tracing::debug!(
        "Submitting feedback for request {} with rating {}",
        key,
        rating
    );
    ctx.client
        .post::<(), _>(&path, &body)
        .await
        .with_context(|| format!("Failed to submit feedback for request {key}"))?;

    tracing::info!(
        "Submitted feedback for request {} with rating {}",
        key,
        rating
    );
    println!("Successfully submitted feedback for request {}", key);
    Ok(())
}

/// Delete feedback for a request.
pub async fn delete_feedback(ctx: &JsmContext<'_>, key: &str) -> Result<()> {
    let path = format!("/rest/servicedeskapi/request/{key}/feedback");

    tracing::debug!("Deleting feedback for request {}", key);
    ctx.client
        .delete::<()>(&path)
        .await
        .with_context(|| format!("Failed to delete feedback for request {key}"))?;

    tracing::info!("Deleted feedback for request {}", key);
    println!("Successfully deleted feedback for request {}", key);
    Ok(())
}
