mod store;
#[cfg(test)]
mod tests;

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

#[derive(Deserialize)]
pub(crate) struct Flag {
    kind: String,
    request_id: i64,
    tool_use_id: Option<i64>,
    reason: String,
    evidence: String,
    actor: String,
}

#[derive(Deserialize)]
pub(crate) struct Review {
    status: String,
    note: String,
    actor: String,
}

fn invalid(message: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"error": message})))
}

fn directory() -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("MIKE_COMPRESSION_QUEUE") {
        return Ok(PathBuf::from(path));
    }
    let home = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("Home directory unavailable"))?;
    Ok(home.home_dir().join(".local/state/mike-compression-review"))
}

async fn run(action: impl FnOnce(PathBuf) -> anyhow::Result<Value> + Send + 'static) -> ApiResult {
    let result = tokio::task::spawn_blocking(move || action(directory()?)).await;
    match result {
        Ok(Ok(value)) if value.get("error").is_some() => Err((StatusCode::NOT_FOUND, Json(value))),
        Ok(Ok(value)) => Ok(Json(value)),
        error => {
            tracing::warn!(?error, "Compression review queue unavailable");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "Compression review queue unavailable"})),
            ))
        }
    }
}

pub(crate) async fn list() -> ApiResult {
    run(|path| {
        let file = store::locked(&path)?;
        Ok(json!(store::items(&store::events(&file)?)?))
    })
    .await
}

fn validate_flag(flag: &Flag) -> Result<(), (StatusCode, Json<Value>)> {
    if !["request", "response", "tool_result"].contains(&flag.kind.as_str()) || flag.request_id <= 0
    {
        return Err(invalid("A valid kind and positive request ID are required"));
    }
    if (flag.kind == "tool_result" && flag.tool_use_id.is_none_or(|id| id <= 0))
        || (flag.kind != "tool_result" && flag.tool_use_id.is_some())
    {
        return Err(invalid(
            "Tool-result flags require a positive database tool-use ID",
        ));
    }
    if [&flag.reason, &flag.evidence, &flag.actor]
        .iter()
        .any(|s| s.trim().is_empty())
    {
        return Err(invalid("Reason, evidence and actor are required"));
    }
    Ok(())
}

pub(crate) async fn flag(Json(flag): Json<Flag>) -> ApiResult {
    validate_flag(&flag)?;
    run(move |path| {
        let mut file = store::locked(&path)?;
        let events = store::events(&file)?;
        let id = store::target_id(&flag.kind, flag.request_id, flag.tool_use_id);
        let duplicate = events.iter().any(|event| event["action"] == "flag" && event["id"] == id
            && event["reason"] == flag.reason && event["evidence"] == flag.evidence && event["actor"] == flag.actor);
        if !duplicate {
            store::append(&mut file, json!({"action": "flag", "id": id,
                "target": {"kind": flag.kind, "request_id": flag.request_id, "tool_use_id": flag.tool_use_id},
                "reason": flag.reason, "evidence": flag.evidence, "actor": flag.actor}))?;
        }
        Ok(json!({"id": id, "added": !duplicate}))
    }).await
}

pub(crate) async fn review(Path(id): Path<String>, Json(review): Json<Review>) -> ApiResult {
    if !["open", "investigating", "evaluated", "dismissed"].contains(&review.status.as_str())
        || review.note.trim().is_empty()
        || review.actor.trim().is_empty()
    {
        return Err(invalid("Valid status, note and actor are required"));
    }
    run(move |path| {
        let mut file = store::locked(&path)?;
        let events = store::events(&file)?;
        if !store::items(&events)?.iter().any(|item| item["id"] == id) {
            return Ok(json!({"error": "Flag no longer exists"}));
        }
        store::append(
            &mut file,
            json!({"action": "review", "id": id,
            "status": review.status, "note": review.note, "actor": review.actor}),
        )?;
        Ok(json!({"id": id, "status": review.status}))
    })
    .await
}
