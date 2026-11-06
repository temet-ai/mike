use crate::db_helpers;

use super::fixtures::{patch_embedding_setting, selective_failure_responder};
use super::harness::start_proxy_with_state;
use super::seed::{fetch_embedding_row, seed_chunk, seed_closed_turn, seed_project, seed_session};

use chrono::Utc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

#[tokio::test]
async fn sweep_continues_past_a_failing_candidate_to_embed_the_rest() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream_mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    const FAIL_MARKER: &str = "TRIGGER_SYNTHETIC_EMBED_FAILURE";
    let infinity_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(selective_failure_responder(FAIL_MARKER))
        .mount(&infinity_mock)
        .await;

    let project_id = seed_project(&pool, "sweep-mixed-outcome-test").await?;

    let (failing_pk, failing_uuid) = seed_session(&pool, project_id).await?;
    let failing_turn = seed_closed_turn(&pool, failing_pk, project_id).await?;
    seed_chunk(&pool, failing_turn, 0).await?;
    sqlx::query(
        "UPDATE sessions.sessions SET overview = $2, overview_generated_at = $3 WHERE id = $1",
    )
    .bind(failing_pk)
    .bind(format!(
        "{FAIL_MARKER}: this session's overview embed call fails."
    ))
    .bind(Utc::now())
    .execute(&pool)
    .await?;

    let (ok_pk, ok_uuid) = seed_session(&pool, project_id).await?;
    let ok_turn = seed_closed_turn(&pool, ok_pk, project_id).await?;
    seed_chunk(&pool, ok_turn, 0).await?;
    sqlx::query(
        "UPDATE sessions.sessions SET overview = $2, overview_generated_at = $3 WHERE id = $1",
    )
    .bind(ok_pk)
    .bind("A normal session overview with no marker text in it.")
    .bind(Utc::now())
    .execute(&pool)
    .await?;

    let (inspect_url2, shutdown2, state) =
        start_proxy_with_state(&upstream_mock.uri(), &test_db.database_url).await?;
    patch_embedding_setting(&inspect_url2, "embedding.enabled", serde_json::json!(true)).await?;
    patch_embedding_setting(
        &inspect_url2,
        "embedding.url",
        serde_json::json!(infinity_mock.uri()),
    )
    .await?;

    let outcome = mike::embed_worker::run_session_vector_sweep_once(&state)
        .await?
        .ok_or_else(|| anyhow::anyhow!("persistence is enabled; sweep must run"))?;

    assert_eq!(
        outcome.failed, 1,
        "exactly the marked candidate's embed call must fail"
    );
    assert_eq!(
        outcome.overview_written, 1,
        "the non-failing candidate must still get its overview-sourced row \
         — a sweep that stopped at the first failure would leave this at 0"
    );

    let failing_row = fetch_embedding_row(&pool, failing_uuid).await?;
    assert!(
        failing_row.is_none(),
        "the failing candidate must be left with no row so the next sweep retries it"
    );
    let ok_row = fetch_embedding_row(&pool, ok_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the succeeding candidate must have its row written"))?;
    assert_eq!(ok_row.0, "overview");

    shutdown2.cancel();
    test_db.cleanup().await?;
    Ok(())
}
