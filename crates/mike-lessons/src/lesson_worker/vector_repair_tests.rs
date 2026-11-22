use super::vector_repair::EmbeddingWork;
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::lessons::NewLessonWrite;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::{PersistHandle, test_support};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn seed(handle: &PersistHandle, project: i64, text: &str) -> anyhow::Result<i64> {
    Ok(handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id: project,
            kind: "convention",
            text,
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?
        .id)
}

fn service(server: &MockServer) -> EmbeddingService {
    service_with(server, "repair-model")
}

fn service_with(server: &MockServer, model: &str) -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: server.uri(),
        model: model.to_owned(),
        chunk_max_chars: 1500,
    })
}

fn fed(handle: &PersistHandle) -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    crate::change_feed::spawn_change_feed(
        &feed,
        Some(handle),
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

async fn embedding_calls(server: &MockServer) -> anyhow::Result<usize> {
    Ok(server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("request capture unavailable"))?
        .len())
}

async fn respond(server: &MockServer, status: u16) {
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(
            ResponseTemplate::new(status).set_body_json(serde_json::json!({
                "data": [{ "embedding": vec![0.25_f32; 768] }]
            })),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn vector_repair_fills_legacy_and_wrong_model_without_promoting_or_repeating()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = test_support::seed_project(handle.pool(), "vector-repair").await?;
    let legacy = seed(
        &handle,
        project,
        "Legacy accepted lesson with missing embedding",
    )
    .await?;
    sqlx::query("UPDATE memory.lessons SET stage_id = s.id FROM memory.lesson_stages s WHERE memory.lessons.id = $1 AND s.code = 'long_term'")
        .bind(legacy).execute(handle.pool()).await?;
    let outdated = seed(
        &handle,
        project,
        "Accepted lesson embedded with obsolete model",
    )
    .await?;
    handle
        .lessons()
        .promote_lesson_to_daily(outdated, &vec![0.5; 768], "old-model")
        .await?;
    let server = MockServer::start().await;
    respond(&server, 200).await;
    let mut work = EmbeddingWork::new();
    work.repair(&handle, &service(&server)).await;
    assert!(
        handle
            .lessons()
            .pending_lesson_vector_repairs("repair-model", 0, 16)
            .await?
            .is_empty()
    );
    let stage: String = sqlx::query_scalar("SELECT s.code FROM memory.lessons l JOIN memory.lesson_stages s ON s.id = l.stage_id WHERE l.id = $1")
        .bind(legacy).fetch_one(handle.pool()).await?;
    assert_eq!(stage, "long_term");
    work.repair(&handle, &service(&server)).await;
    work.repair(&handle, &service(&server)).await;
    assert_eq!(
        server
            .received_requests()
            .await
            .ok_or_else(|| anyhow::anyhow!("request capture unavailable"))?
            .len(),
        2
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn vector_repair_failure_retains_candidate_and_recovers_after_cursor_wrap()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = test_support::seed_project(handle.pool(), "vector-repair-retry").await?;
    let id = seed(
        &handle,
        project,
        "Operator correction must remain retriable after provider failure",
    )
    .await?;
    handle
        .lessons()
        .promote_lesson_to_daily(id, &vec![0.5; 768], "old-model")
        .await?;
    let server = MockServer::start().await;
    respond(&server, 503).await;
    let embedding = service(&server);
    let mut work = EmbeddingWork::new();
    work.repair(&handle, &embedding).await;
    assert_eq!(
        handle
            .lessons()
            .pending_lesson_vector_repairs("repair-model", 0, 16)
            .await?[0]
            .0,
        id
    );
    server.reset().await;
    respond(&server, 200).await;
    work.repair(&handle, &embedding).await;
    work.repair(&handle, &embedding).await;
    assert!(
        handle
            .lessons()
            .pending_lesson_vector_repairs("repair-model", 0, 16)
            .await?
            .is_empty()
    );
    assert!(
        !handle
            .lessons()
            .write_repaired_lesson_vector(
                id,
                "concurrently edited text",
                &vec![0.5; 768],
                "new-model"
            )
            .await?
    );
    sqlx::query("UPDATE memory.lessons SET stage_id = s.id FROM memory.lesson_stages s WHERE memory.lessons.id = $1 AND s.code = 'raw'").bind(id).execute(handle.pool()).await?;
    assert!(
        !handle
            .lessons()
            .write_repaired_lesson_vector(
                id,
                "Operator correction must remain retriable after provider failure",
                &vec![0.5; 768],
                "new-model"
            )
            .await?
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn vector_repair_excludes_expired_archived_and_unapproved_lessons() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = test_support::seed_project(handle.pool(), "vector-repair-eligibility").await?;
    let expired = seed(&handle, project, "Expired lesson").await?;
    let archived = seed(&handle, project, "Archived lesson").await?;
    let unapproved = seed(&handle, project, "Unapproved judge proposal").await?;
    let raw = seed(
        &handle,
        project,
        "Operator lesson awaiting ordinary consolidation",
    )
    .await?;
    handle
        .lessons()
        .promote_lesson_to_daily(expired, &vec![0.5; 768], "old-model")
        .await?;
    sqlx::query("UPDATE memory.lessons SET expires_at = NOW() - interval '1 day' WHERE id = $1")
        .bind(expired)
        .execute(handle.pool())
        .await?;
    handle
        .lessons()
        .promote_lesson_to_daily(archived, &vec![0.5; 768], "old-model")
        .await?;
    handle
        .lessons()
        .archive_lesson(archived, "expired", None)
        .await?;
    sqlx::query("UPDATE memory.lessons SET source_id = s.id FROM memory.lesson_sources s WHERE memory.lessons.id = $1 AND s.code = 'judge'")
        .bind(unapproved).execute(handle.pool()).await?;
    assert!(
        handle
            .lessons()
            .pending_lesson_vector_repairs("repair-model", 0, 16)
            .await?
            .is_empty()
    );
    let server = MockServer::start().await;
    let mut work = EmbeddingWork::new();
    work.repair(&handle, &service(&server)).await;
    assert!(
        handle
            .lessons()
            .claim_raw_lessons_for_consolidation(project, 50)
            .await?
            .iter()
            .any(|lesson| lesson.id == raw)
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn vector_repair_after_finding_none_stays_quiet_until_lessons_change() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let feed = fed(&handle);
    let project = test_support::seed_project(handle.pool(), "vector-repair-quiet").await?;
    let server = MockServer::start().await;
    respond(&server, 200).await;
    let mut work = EmbeddingWork::new();
    work.repair(&handle, &service(&server)).await;
    let id = seed(
        &handle,
        project,
        "Lesson promoted where the feed did not hear it",
    )
    .await?;
    handle
        .lessons()
        .promote_lesson_to_daily(id, &vec![0.5; 768], "old-model")
        .await?;
    work.repair(&handle, &service(&server)).await;
    feed.record(RowChangedTable::Requests, None);
    work.repair(&handle, &service(&server)).await;
    assert_eq!(
        embedding_calls(&server).await?,
        0,
        "no lesson change reached the feed"
    );
    feed.record(RowChangedTable::Lessons, None);
    work.repair(&handle, &service(&server)).await;
    assert_eq!(embedding_calls(&server).await?, 1);
    assert!(
        handle
            .lessons()
            .pending_lesson_vector_repairs("repair-model", 0, 16)
            .await?
            .is_empty()
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn vector_repair_after_finding_none_runs_again_when_the_embedder_changes()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let _feed = fed(&handle);
    let project = test_support::seed_project(handle.pool(), "vector-repair-model").await?;
    let id = seed(&handle, project, "Lesson embedded by the embedder in use").await?;
    handle
        .lessons()
        .promote_lesson_to_daily(id, &vec![0.5; 768], "repair-model")
        .await?;
    let server = MockServer::start().await;
    respond(&server, 200).await;
    let mut work = EmbeddingWork::new();
    work.repair(&handle, &service(&server)).await;
    work.repair(&handle, &service(&server)).await;
    assert_eq!(embedding_calls(&server).await?, 0);
    work.repair(&handle, &service_with(&server, "next-model"))
        .await;
    assert_eq!(embedding_calls(&server).await?, 1);
    db.cleanup().await?;
    Ok(())
}
