use crate::store::persistence::web_repo::WebPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde_json::{Value, json};

use super::remedies::{
    ActionsQuery, PatchRemedyBody, list_actions, list_remedies, patch_remedy, undo_action,
};
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
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

    fn handle(&self) -> anyhow::Result<&PersistHandle> {
        self.state
            .persist()
            .ok_or_else(|| anyhow::anyhow!("persistence is required"))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }
}

async fn body_of(response: Response) -> anyhow::Result<(StatusCode, Value)> {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

fn answer(result: Result<Json<Value>, Response>) -> Response {
    match result {
        Ok(json) => json.into_response(),
        Err(response) => response,
    }
}

fn patch(value: Value) -> anyhow::Result<Json<PatchRemedyBody>> {
    Ok(Json(serde_json::from_value(value)?))
}

#[tokio::test]
async fn the_seeded_remedies_are_listed_with_their_limits() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (status, body) = body_of(answer(list_remedies(State(env.state.clone())).await)).await?;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body["remedies"]
        .as_array()
        .map(|all| all.iter().filter_map(|r| r["name"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(names, ["close_orphan_launches", "tell_undelivered_senders"]);
    let first = &body["remedies"][0];
    for field in [
        "kind",
        "action",
        "check_name",
        "enabled",
        "daily_limit",
        "max_resources",
        "paused_at",
        "pause_reason",
        "actions_today",
        "ineffective_runs",
    ] {
        assert!(first.get(field).is_some(), "missing {field}");
    }
    env.cleanup().await
}

#[tokio::test]
async fn a_patch_outside_the_range_names_the_value() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let response = answer(
        patch_remedy(
            State(env.state.clone()),
            Path("close_orphan_launches".to_owned()),
            patch(json!({"daily_limit": 0}))?,
        )
        .await,
    );
    let (status, body) = body_of(response).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.contains("daily_limit is 0"))
    );
    assert!(serde_json::from_value::<PatchRemedyBody>(json!({"measure_after_passes": 1})).is_err());
    env.cleanup().await
}

#[tokio::test]
async fn enabling_a_paused_remedy_resumes_it_and_records_the_change() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    sqlx::query(
        "UPDATE wright.remedies SET paused_at = now(), pause_reason = 'paused after 3 refusals',
                consecutive_refusals = 3 WHERE name = 'tell_undelivered_senders'",
    )
    .execute(env.handle()?.pool())
    .await?;
    let response = answer(
        patch_remedy(
            State(env.state.clone()),
            Path("tell_undelivered_senders".to_owned()),
            patch(json!({"enabled": true, "max_resources": 7}))?,
        )
        .await,
    );
    let (status, body) = body_of(response).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["remedy"]["paused_at"], Value::Null);
    assert_eq!(body["remedy"]["max_resources"], 7);
    let changes: Vec<(Value, Value)> = sqlx::query_as(
        "SELECT old_value, new_value FROM wright.remedy_changes WHERE remedy = 'tell_undelivered_senders'",
    )
    .fetch_all(env.handle()?.pool())
    .await?;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0["pause_reason"], "paused after 3 refusals");
    assert_eq!(changes[0].1["max_resources"], 7);

    let missing = answer(
        patch_remedy(
            State(env.state.clone()),
            Path("no_such_fix".to_owned()),
            patch(json!({"enabled": false}))?,
        )
        .await,
    );
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    env.cleanup().await
}

async fn closed_launch_action(env: &Env) -> anyhow::Result<i64> {
    let handle = env.handle()?;
    let recorded = handle
        .web()
        .record_wright_run(&CheckRun {
            check: "orphan_launches",
            source: "capture_db",
            cadence: "regular",
            started_at: Utc::now(),
            outcome: &CheckOutcome::Complete(vec![Finding {
                scope: "launch".to_owned(),
                resource: "424242".to_owned(),
                evidence: json!({"launch_id": 424_242}),
            }]),
            machine: None,
        })
        .await?;
    assert_eq!(recorded.opened, 1);
    let due = handle.web().due_wright_actions(10).await?;
    let item = due
        .iter()
        .find(|d| d.remedy == "close_orphan_launches")
        .ok_or_else(|| anyhow::anyhow!("no due close"))?;
    handle
        .web()
        .claim_wright_action(&item.remedy, item.problem_id, item.episode_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("claim refused"))
}

#[tokio::test]
async fn actions_page_by_id_and_a_claimed_close_cannot_be_undone() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let id = closed_launch_action(&env).await?;
    let query = ActionsQuery {
        problem_id: None,
        limit: Some(1),
        before: None,
    };
    let (status, body) = body_of(answer(
        list_actions(State(env.state.clone()), Query(query)).await,
    ))
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["actions"][0]["id"], id);
    assert_eq!(body["actions"][0]["can_undo"], false);
    assert_eq!(body["actions"][0]["by"], "wright");
    assert_eq!(body["next_before"], id);

    let after = ActionsQuery {
        problem_id: None,
        limit: Some(1),
        before: Some(id),
    };
    let (_, rest) = body_of(answer(
        list_actions(State(env.state.clone()), Query(after)).await,
    ))
    .await?;
    assert_eq!(rest["actions"], json!([]));
    assert_eq!(rest["next_before"], Value::Null);

    let (status, body) = body_of(answer(
        undo_action(State(env.state.clone()), Path(id)).await,
    ))
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "conflict");
    assert!(
        body["reason"]
            .as_str()
            .is_some_and(|r| r.contains("cannot be undone"))
    );
    let unknown = answer(undo_action(State(env.state.clone()), Path(id + 1_000)).await);
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    env.cleanup().await
}
