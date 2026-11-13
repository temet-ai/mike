use crate::db_helpers;

use super::fixtures::{echo_count_responder, patch_embedding_setting};
use super::harness::start_proxy_with_state;
use super::seed::{fetch_embedding_row, seed_chunk, seed_closed_turn, seed_project, seed_session};

use chrono::Utc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

#[tokio::test]
async fn sweep_gives_chunk_only_session_a_centroid_row() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream_mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let infinity_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(echo_count_responder())
        .mount(&infinity_mock)
        .await;

    let project_id = seed_project(&pool, "sweep-centroid-test").await?;
    let (session_pk, session_uuid) = seed_session(&pool, project_id).await?;
    let turn_id = seed_closed_turn(&pool, session_pk, project_id).await?;
    seed_chunk(&pool, turn_id, 0).await?;
    seed_chunk(&pool, turn_id, 1).await?;

    let (inspect_url2, shutdown2, state) =
        start_proxy_with_state(&upstream_mock.uri(), &test_db.database_url).await?;
    patch_embedding_setting(&inspect_url2, "embedding.enabled", serde_json::json!(true)).await?;
    patch_embedding_setting(
        &inspect_url2,
        "embedding.url",
        serde_json::json!(infinity_mock.uri()),
    )
    .await?;

    let outcome = mike::embed_worker::run_session_vector_sweep_once(&state.knowledge_state())
        .await?
        .ok_or_else(|| anyhow::anyhow!("persistence is enabled; sweep must run"))?;
    assert_eq!(
        outcome.centroid_written, 1,
        "one session must get a centroid row"
    );
    assert_eq!(outcome.overview_written, 0);

    let row = fetch_embedding_row(&pool, session_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session_embeddings row must exist after the sweep"))?;
    assert_eq!(row.0, "centroid");
    assert_eq!(row.1, 2, "chunk_count must reflect the two seeded chunks");

    shutdown2.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sweep_gives_overview_session_an_overview_sourced_row() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream_mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let infinity_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(echo_count_responder())
        .mount(&infinity_mock)
        .await;

    let project_id = seed_project(&pool, "sweep-overview-test").await?;
    let (session_pk, session_uuid) = seed_session(&pool, project_id).await?;
    let turn_id = seed_closed_turn(&pool, session_pk, project_id).await?;
    seed_chunk(&pool, turn_id, 0).await?;
    seed_chunk(&pool, turn_id, 1).await?;
    seed_chunk(&pool, turn_id, 2).await?;

    sqlx::query(
        "UPDATE sessions.sessions
            SET overview = 'The session fixed a bug.\n\nThe fix shipped.',
                overview_generated_at = $2
          WHERE id = $1",
    )
    .bind(session_pk)
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

    let outcome = mike::embed_worker::run_session_vector_sweep_once(&state.knowledge_state())
        .await?
        .ok_or_else(|| anyhow::anyhow!("persistence is enabled; sweep must run"))?;
    assert_eq!(
        outcome.overview_written, 1,
        "one session must get an overview row"
    );
    assert_eq!(outcome.centroid_written, 0);

    let row = fetch_embedding_row(&pool, session_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session_embeddings row must exist after the sweep"))?;
    assert_eq!(row.0, "overview");
    assert_eq!(
        row.1, 3,
        "chunk_count must reflect the three seeded chunks even though the source is the overview"
    );

    shutdown2.cancel();
    test_db.cleanup().await?;
    Ok(())
}
