use crate::store::persistence::lessons_repo::LessonsPersist as _;
pub mod fixtures;

use fixtures::{
    ActionRow, assert_full_decision_evidence, embedding_service_returning, lesson_stage,
    seed_nearest_and_runners_up, seed_skip_cause_embed_http_error,
    seed_skip_cause_empty_embed_response, seed_skip_cause_vector_search_error, unit_vec,
};

use crate::lesson_worker::session_consolidation::consolidate_project;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::NewLessonWrite;
use crate::store::persistence::test_support;
use sqlx::postgres::PgPoolOptions;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn near_duplicate_raw_lesson_is_promoted_not_archived() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, "consolidation-near-dup").await?;

    let existing = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text: "existing daily lesson",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    handle
        .lessons()
        .promote_lesson_to_daily(existing.id, &unit_vec(0), "test-model")
        .await?;

    let incoming = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text: "incoming raw lesson",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;

    let mut near_e0 = unit_vec(0);
    near_e0[0] = 0.999;
    near_e0[1] = 0.0447;
    let (_server, embedding) = embedding_service_returning(near_e0).await;

    consolidate_project(&handle, &embedding, project_id, 0.85, None).await;

    let (stage, archived_at) = lesson_stage(&pool, incoming.id).await?;
    assert_eq!(
        stage, "daily",
        "a near-duplicate raw lesson must still be promoted to daily"
    );
    assert!(
        archived_at.is_none(),
        "similarity must never archive the incoming lesson"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_lesson_without_neighbour_is_promoted() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, "consolidation-no-neighbour").await?;

    let incoming = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text: "solo raw lesson",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;

    let (_server, embedding) = embedding_service_returning(unit_vec(5)).await;

    consolidate_project(&handle, &embedding, project_id, 0.85, None).await;

    let (stage, archived_at) = lesson_stage(&pool, incoming.id).await?;
    assert_eq!(
        stage, "daily",
        "a lesson with no neighbour must still be promoted"
    );
    assert!(archived_at.is_none());

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn archive_lesson_still_archives_independently_of_consolidation() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, "consolidation-archive-path").await?;
    let lesson = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text: "lesson to expire",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    handle
        .lessons()
        .promote_lesson_to_daily(lesson.id, &unit_vec(1), "test-model")
        .await?;

    handle
        .lessons()
        .archive_lesson(lesson.id, "expired", None)
        .await?;

    let (stage, archived_at) = lesson_stage(&pool, lesson.id).await?;
    assert_eq!(
        stage, "archived",
        "archive_lesson must still soft-archive independently of dedup"
    );
    assert!(archived_at.is_some());

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_consolidation_records_full_decision_evidence() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, "full-decision-evidence").await?;

    let nearest_text = "nearest neighbour lesson";
    seed_nearest_and_runners_up(&handle, project_id, nearest_text).await?;

    let incoming_text = "incoming raw lesson under evaluation";
    let incoming = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text: incoming_text,
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;

    let mut near_e0 = unit_vec(0);
    near_e0[0] = 0.999;
    near_e0[1] = 0.0447;
    let (_server, embedding) = embedding_service_returning(near_e0).await;

    let run_id = handle
        .lessons()
        .start_consolidation_run("session", "manual")
        .await?;
    consolidate_project(&handle, &embedding, project_id, 0.85, Some(run_id)).await;

    let row = sqlx::query_as!(
        ActionRow,
        "SELECT lesson_text, distance, threshold, counterpart_text, runners_up
           FROM memory.consolidation_actions
          WHERE run_id = $1 AND lesson_id = $2",
        run_id,
        incoming.id,
    )
    .fetch_one(&pool)
    .await?;

    assert_full_decision_evidence(&row, incoming_text, nearest_text)?;

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_consolidation_skip_causes_are_distinguishable() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let run_id = handle
        .lessons()
        .start_consolidation_run("session", "manual")
        .await?;

    let lesson_empty_id = seed_skip_cause_empty_embed_response(&handle, &pool, run_id).await?;
    let lesson_err_id = seed_skip_cause_embed_http_error(&handle, &pool, run_id).await?;
    let lesson_dim_id = seed_skip_cause_vector_search_error(&handle, &pool, run_id).await?;

    let lesson_ids = vec![lesson_empty_id, lesson_err_id, lesson_dim_id];
    let rows = sqlx::query!(
        "SELECT lesson_id, error FROM memory.consolidation_actions
          WHERE run_id = $1 AND lesson_id = ANY($2)
          ORDER BY lesson_id",
        run_id,
        &lesson_ids,
    )
    .fetch_all(&pool)
    .await?;

    assert_eq!(rows.len(), 3, "all three skip causes must be recorded");
    let errors: std::collections::HashSet<String> =
        rows.into_iter().filter_map(|row| row.error).collect();
    assert_eq!(
        errors.len(),
        3,
        "the three skip causes must record three DISTINCT error strings, got: {errors:?}"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
