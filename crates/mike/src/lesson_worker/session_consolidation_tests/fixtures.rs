use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::lesson_worker::session_consolidation::consolidate_project;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::NewLessonWrite;
use crate::store::persistence::test_support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(super) fn unit_vec(slot: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; 768];
    v[slot] = 1.0;
    v
}

fn infinity_response(embedding: Vec<f32>) -> serde_json::Value {
    serde_json::json!({
        "data": [{ "embedding": embedding, "index": 0, "object": "embedding" }]
    })
}

pub(super) async fn embedding_service_returning(
    embedding: Vec<f32>,
) -> (MockServer, EmbeddingService) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(infinity_response(embedding)))
        .mount(&server)
        .await;
    let service = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: server.uri(),
        model: "test-model".to_string(),
        chunk_max_chars: 1500,
    });
    (server, service)
}

pub(super) async fn lesson_stage(
    pool: &sqlx::PgPool,
    lesson_id: i64,
) -> anyhow::Result<(String, Option<chrono::DateTime<chrono::Utc>>)> {
    let row = sqlx::query!(
        "SELECT ls.code, l.archived_at FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.id = $1",
        lesson_id,
    )
    .fetch_one(pool)
    .await?;
    Ok((row.code, row.archived_at))
}

pub(super) struct ActionRow {
    pub(super) lesson_text: String,
    pub(super) distance: Option<f32>,
    pub(super) threshold: Option<f32>,
    pub(super) counterpart_text: Option<String>,
    pub(super) runners_up: serde_json::Value,
}

pub(super) async fn seed_nearest_and_runners_up(
    handle: &PersistHandle,
    project_id: i64,
    nearest_text: &str,
) -> anyhow::Result<i64> {
    let nearest_id = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text: nearest_text,
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?
        .id;
    handle
        .promote_lesson_to_daily(nearest_id, &unit_vec(0), "test-model")
        .await?;
    for (slot, text) in [(1, "runner-up A"), (2, "runner-up B")] {
        let id = handle
            .create_lesson_with_topic(NewLessonWrite {
                project_id,
                kind: "convention",
                text,
                topic: None,
                source_turn_id: None,
                agent_id: None,
                session_db_id: None,
                request_id: None,
            })
            .await?
            .id;
        handle
            .promote_lesson_to_daily(id, &unit_vec(slot), "test-model")
            .await?;
    }
    Ok(nearest_id)
}

pub(super) fn assert_full_decision_evidence(
    row: &ActionRow,
    incoming_text: &str,
    nearest_text: &str,
) -> anyhow::Result<()> {
    assert_eq!(
        row.lesson_text, incoming_text,
        "must carry the full lesson text"
    );
    let distance = row
        .distance
        .ok_or_else(|| anyhow::anyhow!("a promoted-with-neighbour row must record distance"))?;
    assert!(
        distance < 0.15,
        "distance must reflect the near-identical probe: {distance}"
    );
    let threshold = row
        .threshold
        .ok_or_else(|| anyhow::anyhow!("must record the threshold in effect"))?;
    assert!(
        (threshold - 0.15).abs() < 1e-6,
        "threshold must be 1.0 - dedup_threshold (0.85)"
    );
    assert_eq!(
        row.counterpart_text.as_deref(),
        Some(nearest_text),
        "must carry the counterpart's own text, not just its id"
    );
    let runners_up = row
        .runners_up
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("runners_up must be a JSON array"))?;
    assert_eq!(
        runners_up.len(),
        2,
        "ranks 2-3 of the 3-candidate neighbour set must both be recorded: {:?}",
        row.runners_up
    );
    Ok(())
}

pub(super) async fn seed_skip_cause_empty_embed_response(
    handle: &PersistHandle,
    pool: &sqlx::PgPool,
    run_id: i64,
) -> anyhow::Result<i64> {
    let project_empty = test_support::seed_project(pool, "skip-cause-empty").await?;
    let empty_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
        .mount(&empty_server)
        .await;
    let empty_embedding = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: empty_server.uri(),
        model: "test-model".to_string(),
        chunk_max_chars: 1500,
    });
    let lesson_empty = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id: project_empty,
            kind: "convention",
            text: "cause: empty embed response",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    consolidate_project(handle, &empty_embedding, project_empty, 0.85, Some(run_id)).await;
    Ok(lesson_empty.id)
}

pub(super) async fn seed_skip_cause_embed_http_error(
    handle: &PersistHandle,
    pool: &sqlx::PgPool,
    run_id: i64,
) -> anyhow::Result<i64> {
    let project_err = test_support::seed_project(pool, "skip-cause-http-error").await?;
    let error_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&error_server)
        .await;
    let error_embedding = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: error_server.uri(),
        model: "test-model".to_string(),
        chunk_max_chars: 1500,
    });
    let lesson_err = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id: project_err,
            kind: "convention",
            text: "cause: embed http error",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    consolidate_project(handle, &error_embedding, project_err, 0.85, Some(run_id)).await;
    Ok(lesson_err.id)
}

pub(super) async fn seed_skip_cause_vector_search_error(
    handle: &PersistHandle,
    pool: &sqlx::PgPool,
    run_id: i64,
) -> anyhow::Result<i64> {
    let project_dim = test_support::seed_project(pool, "skip-cause-dim-mismatch").await?;
    let dim_mismatch_neighbour = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id: project_dim,
            kind: "convention",
            text: "existing embedded neighbour",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    handle
        .promote_lesson_to_daily(dim_mismatch_neighbour.id, &unit_vec(0), "test-model")
        .await?;
    let wrong_dim = vec![1.0_f32; 5];
    let (_dim_server, dim_embedding) = embedding_service_returning(wrong_dim).await;
    let lesson_dim = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id: project_dim,
            kind: "convention",
            text: "cause: vector search error",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    consolidate_project(handle, &dim_embedding, project_dim, 0.85, Some(run_id)).await;
    Ok(lesson_dim.id)
}
