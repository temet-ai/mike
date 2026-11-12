use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;

use super::post_run_scheduled_job;
use crate::config::ProxyConfig;
use crate::scheduled_jobs::{JobStatus, replace_for_tests, snapshot};
use crate::state::AppState;

fn from_app() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://127.0.0.1:9990"),
    );
    headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:9989"));
    headers
}

fn ready() -> JobStatus {
    JobStatus {
        started: true,
        enabled: true,
        root: Some(PathBuf::from("/srv/mike")),
        ..JobStatus::default()
    }
}

async fn post(headers: HeaderMap, name: &str) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let response: Response =
        post_run_scheduled_job(State(state), Path(name.to_owned()), headers).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

#[tokio::test]
async fn run_now_without_the_app_origin_is_forbidden_and_queues_nothing() -> anyhow::Result<()> {
    replace_for_tests("learning_promote", &ready());
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:9989"));
    let (status, body) = post(headers, "learning_promote").await?;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["reason"], "not_from_app");
    assert_eq!(snapshot("learning_promote").run_requested_at, None);
    Ok(())
}

#[tokio::test]
async fn run_now_while_the_job_runs_is_a_conflict_saying_so() -> anyhow::Result<()> {
    let mut running = ready();
    running.running_since = Some(chrono::Utc::now());
    replace_for_tests("cargo_sweep", &running);
    let (status, body) = post(from_app(), "cargo_sweep").await?;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["reason"], "already_running");
    let message = body["error"].as_str().unwrap_or_default();
    assert!(message.contains("already running"), "{body}");
    Ok(())
}

#[tokio::test]
async fn run_now_for_a_turned_off_job_is_a_conflict_saying_it_is_off() -> anyhow::Result<()> {
    let mut off = ready();
    off.enabled = false;
    replace_for_tests("compressor_loop", &off);
    let (status, body) = post(from_app(), "compressor_loop").await?;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["reason"], "turned_off");
    Ok(())
}

#[tokio::test]
async fn run_now_for_an_unknown_job_is_not_found_and_names_it() -> anyhow::Result<()> {
    let (status, body) = post(from_app(), "no_such_job").await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["reason"], "unknown_job");
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("no_such_job"),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn run_now_from_the_app_is_accepted_and_queued_for_the_worker() -> anyhow::Result<()> {
    replace_for_tests("lessons_commit", &ready());
    let (status, body) = post(from_app(), "lessons_commit").await?;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["job"], "lessons_commit");
    assert_eq!(body["already_requested"], false);
    assert!(body["run_requested_at"].is_string(), "{body}");
    assert!(snapshot("lessons_commit").run_requested_at.is_some());
    Ok(())
}
