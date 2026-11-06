use crate::db_helpers;

use super::fixtures::{echo_count_responder, patch_embedding_setting};
use super::harness::start_proxy_with_state;
use super::seed::{seed_chunk, seed_closed_turn, seed_project, seed_session};

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

#[tokio::test]
async fn sweep_refreshes_project_centroids_end_to_end() -> anyhow::Result<()> {
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

    let project_id = seed_project(&pool, "sweep-project-centroid-test").await?;
    let (session_pk, _session_uuid) = seed_session(&pool, project_id).await?;
    let turn_id = seed_closed_turn(&pool, session_pk, project_id).await?;
    seed_chunk(&pool, turn_id, 0).await?;

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
    assert_eq!(outcome.centroid_written, 1);
    assert_eq!(
        outcome.projects_refreshed, 1,
        "the sweep's own call to refresh_project_centroids must count this project"
    );

    let row: Option<(i32,)> =
        sqlx::query_as("SELECT session_count FROM memory.project_embeddings WHERE project_id = $1")
            .bind(project_id)
            .fetch_optional(&pool)
            .await?;
    let (session_count,) = row.ok_or_else(|| {
        anyhow::anyhow!("memory.project_embeddings must have a row for the project after the sweep")
    })?;
    assert_eq!(
        session_count, 1,
        "session_count must reflect the one session vector folded into the centroid"
    );

    shutdown2.cancel();
    test_db.cleanup().await?;
    Ok(())
}
