use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::checks::{
    AddCheckBody, PatchCheckBody, add_check, delete_check, list_checks, patch_check,
};
use crate::config::{AppState, ProxyConfig};
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::wright::ProblemFilter;
use crate::wright::{CheckOutcome, CheckRun, Finding};

struct Env {
    db: TestDbHandle,
    state: AppState,
}

impl Env {
    async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            "https://api.anthropic.com",
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:0",
            "--persist-enabled",
            "1",
            "--database-url",
            &db.database_url,
        ])?;
        let state = AppState::new(config).await?;
        Ok(Self { db, state })
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let handle = self
            .state
            .persist()
            .ok_or_else(|| anyhow::anyhow!("persistence is required"))?;
        Ok(handle.list_wright_checks().await?.len())
    }
}

fn add_body(value: Value) -> anyhow::Result<AddCheckBody> {
    Ok(serde_json::from_value(value)?)
}

fn patch_body(value: Value) -> anyhow::Result<PatchCheckBody> {
    Ok(serde_json::from_value(value)?)
}

async fn status_and_body(response: Response) -> anyhow::Result<(StatusCode, Value)> {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

fn sql_check(name: &str, query: &str) -> Value {
    json!({
        "name": name,
        "source": "workers",
        "cadence": "regular",
        "expects": "No scheduled job has failed twice in a row",
        "probe": "sql",
        "query": query,
        "params": {"min_failures": 2},
    })
}

const GOOD_QUERY: &str =
    "SELECT 'job'::text, job_name, jsonb_build_object('failures', consecutive_failures)
  FROM jobs.runs WHERE consecutive_failures >= ($1->>'min_failures')::int LIMIT 200";

#[tokio::test]
async fn a_check_is_added_changed_disabled_listed_and_removed() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    assert_eq!(env.count().await?, 11);

    let added = add_check(
        State(env.state.clone()),
        Json(add_body(sql_check("job_failures_twice", GOOD_QUERY))?),
    )
    .await;
    let (status, body) = status_and_body(added.unwrap_or_else(|r| r)).await?;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["check"]["enabled"], true);
    assert_eq!(env.count().await?, 12);

    let again = add_check(
        State(env.state.clone()),
        Json(add_body(sql_check("job_failures_twice", GOOD_QUERY))?),
    )
    .await;
    let (status, _) = status_and_body(again.unwrap_or_else(|r| r)).await?;
    assert_eq!(status, StatusCode::CONFLICT);

    let Json(patched) = patch_check(
        State(env.state.clone()),
        Path("job_failures_twice".to_owned()),
        Json(patch_body(json!({
            "expects": "No job fails three times",
            "params": {"min_failures": 3},
            "cadence": "daily",
            "enabled": false,
        }))?),
    )
    .await
    .map_err(|_| anyhow::anyhow!("patch must succeed"))?;
    assert_eq!(patched["check"]["params"], json!({"min_failures": 3}));
    assert_eq!(patched["check"]["cadence"], "daily");
    assert_eq!(patched["check"]["enabled"], false);

    let Json(listed) = list_checks(State(env.state.clone()))
        .await
        .map_err(|_| anyhow::anyhow!("list must succeed"))?;
    let checks = listed["checks"].as_array().cloned().unwrap_or_default();
    assert_eq!(checks.len(), 12);
    assert!(
        checks
            .iter()
            .any(|c| c["name"] == "job_failures_twice" && c["enabled"] == false)
    );

    let Json(deleted) = delete_check(
        State(env.state.clone()),
        Path("job_failures_twice".to_owned()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("delete must succeed"))?;
    assert_eq!(deleted["deleted"], true);
    let missing = delete_check(
        State(env.state.clone()),
        Path("job_failures_twice".to_owned()),
    )
    .await;
    let (status, _) = status_and_body(missing.err().ok_or_else(|| anyhow::anyhow!("404"))?).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(env.count().await?, 11);
    env.cleanup().await
}

#[tokio::test]
async fn a_query_the_database_refuses_is_a_400_and_writes_nothing() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let broken = add_check(
        State(env.state.clone()),
        Json(add_body(sql_check(
            "broken_check",
            "SELECT * FROM wright.no_such_table",
        ))?),
    )
    .await;
    let (status, body) =
        status_and_body(broken.err().ok_or_else(|| anyhow::anyhow!("400"))?).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.contains("no_such_table")),
        "{body}"
    );

    let wrong_columns = add_check(
        State(env.state.clone()),
        Json(add_body(sql_check(
            "two_columns",
            "SELECT 'a'::text, 'b'::text",
        ))?),
    )
    .await;
    let (status, body) =
        status_and_body(wrong_columns.err().ok_or_else(|| anyhow::anyhow!("400"))?).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.contains("(text, text, jsonb)")),
        "{body}"
    );
    assert_eq!(env.count().await?, 11);
    env.cleanup().await
}

#[tokio::test]
async fn invalid_definitions_name_the_offending_input() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let mut bad_name = sql_check("Bad Name", GOOD_QUERY);
    bad_name["name"] = json!("Bad Name");
    let mut bad_logs = json!({
        "name": "noisy_logs", "source": "logs", "cadence": "regular",
        "expects": "Quiet", "probe": "logs",
        "params": {"level": "WARN", "repeats": 0, "window_minutes": 2000},
    });
    let bad_cadence = {
        let mut c = sql_check("odd_cadence", GOOD_QUERY);
        c["cadence"] = json!("hourly");
        c
    };
    for (body, needle) in [
        (bad_name, "Bad Name"),
        (bad_cadence, "hourly"),
        (bad_logs.clone(), "repeats"),
    ] {
        let refused = add_check(State(env.state.clone()), Json(add_body(body)?)).await;
        let (status, answer) =
            status_and_body(refused.err().ok_or_else(|| anyhow::anyhow!("400"))?).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            answer["error"].as_str().is_some_and(|e| e.contains(needle)),
            "{answer}"
        );
    }
    bad_logs["params"] = json!({"level": "WARN", "repeats": 5, "window_minutes": 2000});
    let refused = add_check(State(env.state.clone()), Json(add_body(bad_logs)?)).await;
    let (_, answer) = status_and_body(refused.err().ok_or_else(|| anyhow::anyhow!("400"))?).await?;
    assert!(
        answer["error"]
            .as_str()
            .is_some_and(|e| e.contains("window_minutes") && e.contains("2000"))
    );
    assert_eq!(env.count().await?, 11);
    env.cleanup().await
}

#[tokio::test]
async fn removing_a_check_removes_its_problems_and_runs_but_disabling_keeps_them()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let handle = env
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence is required"))?;
    handle
        .record_wright_run(&CheckRun {
            check: "request_failures",
            source: "capture_db",
            cadence: "event",
            started_at: chrono::Utc::now(),
            outcome: &CheckOutcome::Complete(vec![Finding {
                scope: "capture".to_owned(),
                resource: "stream aborted".to_owned(),
                evidence: json!({"failed_requests": 5}),
            }]),
            machine: None,
        })
        .await?;
    let filter = ProblemFilter {
        limit: 10,
        ..ProblemFilter::default()
    };
    assert_eq!(handle.list_wright_problems(&filter).await?.total, 1);

    let Json(_) = patch_check(
        State(env.state.clone()),
        Path("request_failures".to_owned()),
        Json(patch_body(json!({"enabled": false}))?),
    )
    .await
    .map_err(|_| anyhow::anyhow!("disable must succeed"))?;
    assert_eq!(handle.list_wright_problems(&filter).await?.total, 1);
    assert_eq!(handle.latest_wright_runs().await?.len(), 1);

    let Json(_) = delete_check(
        State(env.state.clone()),
        Path("request_failures".to_owned()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("delete must succeed"))?;
    assert_eq!(handle.list_wright_problems(&filter).await?.total, 0);
    assert!(handle.latest_wright_runs().await?.is_empty());
    env.cleanup().await
}

#[tokio::test]
async fn params_patch_merges_and_refuses_dropping_a_key_the_query_reads() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let mut no_params = sql_check("job_failures_unread", GOOD_QUERY);
    no_params["params"] = json!({});
    let refused = add_check(State(env.state.clone()), Json(add_body(no_params)?)).await;
    let (status, body) = status_and_body(refused.unwrap_or_else(|r| r)).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .contains("min_failures")
    );
    assert_eq!(env.count().await?, 11);

    let Json(merged) = patch_check(
        State(env.state.clone()),
        Path("mcp_tool_errors".to_owned()),
        Json(patch_body(json!({"params": {"min_sessions": 5}}))?),
    )
    .await
    .map_err(|_| anyhow::anyhow!("a one-key params patch must succeed"))?;
    assert_eq!(merged["check"]["params"]["min_sessions"], 5);
    assert_eq!(merged["check"]["params"]["min_failures"], 20);
    assert_eq!(merged["check"]["params"]["window_hours"], 24);

    let dropped = patch_check(
        State(env.state.clone()),
        Path("mcp_tool_errors".to_owned()),
        Json(patch_body(json!({"params": {"min_failures": null}}))?),
    )
    .await;
    let (status, body) = status_and_body(
        dropped
            .map(IntoResponse::into_response)
            .unwrap_or_else(|r| r),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .contains("min_failures")
    );
    env.cleanup().await
}
