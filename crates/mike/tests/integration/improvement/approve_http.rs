use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;

use mike::api::cross_session_approve::{ApproveBody, approve_proposal};
use mike::config::ProxyConfig;
use mike::state::AppState;
use mike::store::test_support;

use crate::test_db::TestDb;

use super::open_directive_proposal;

async fn persisted_test_state()
-> anyhow::Result<Option<(mike::store::test_support::TestDbHandle, AppState)>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire_isolated().await?;
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
    Ok(Some((db, state)))
}

#[tokio::test]
async fn approve_through_the_http_handler_logs_a_directive_decision_change() -> anyhow::Result<()> {
    let Some((_db, state)) = persisted_test_state().await? else {
        return Ok(());
    };
    let persist = state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence is enabled"))?;
    let project_id = test_support::seed_project(persist.pool(), "improvement-approve-http").await?;
    let proposal_id = open_directive_proposal(persist.pool(), project_id).await?;

    let body = ApproveBody {
        text: "commit only after the tests are green".to_owned(),
        kind: "correction".to_owned(),
        target_project_ids: vec![project_id],
        expires_at: None,
    };
    let response = approve_proposal(
        State(state.clone()),
        HeaderMap::new(),
        Path(proposal_id),
        Json(body),
    )
    .await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "the approve handler must succeed for a fresh open proposal"
    );

    let change_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM config.change_log
          WHERE subject_kind = 'directive_decision' AND subject_key = $1",
    )
    .bind(format!("proposal:{proposal_id}"))
    .fetch_one(persist.pool())
    .await?;
    assert_eq!(
        change_rows, 1,
        "the HTTP handler must log exactly one directive_decision change row"
    );

    let lesson_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.directive_lessons WHERE proposal_id = $1")
            .bind(proposal_id)
            .fetch_one(persist.pool())
            .await?;
    assert_eq!(lesson_count, 1);
    Ok(())
}
