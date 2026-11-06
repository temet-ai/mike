use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::test_support;
use mike::store::persistence::{LessonRelationDraft, RelationDecisionOutcome};
use sqlx::postgres::PgPoolOptions;

use crate::test_db::TestDb;

async fn seed_two_lessons(handle: &PersistHandle, project_id: i64) -> anyhow::Result<(i64, i64)> {
    let older = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "Cross-session mailbox delivery can land in SUBAGENT requests, not the main loop.",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    let newer = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "RESOLVED: mailbox delivery landing on SUBAGENT requests is FIXED. Root cause: extract_turn_origin.",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    Ok((older.id, newer.id))
}

fn draft(from_lesson_id: i64, to_lesson_id: i64) -> LessonRelationDraft<'static> {
    LessonRelationDraft {
        from_lesson_id,
        to_lesson_id,
        kind: "supersedes",
        method: "declared",
        status: "proposed",
        candidate_distance: Some(0.1052),
        evidence_json: serde_json::json!({}),
        proposer_model: None,
        proposer_rationale: None,
        adjudicator_model: None,
        adjudicator_agrees: None,
        adjudicator_rationale: None,
        detector_version: "test",
        decided_by: None,
    }
}

#[tokio::test]
async fn confirm_and_reject_relation_missing_id_return_not_found() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let confirm_outcome = handle.confirm_relation(999_999_999, "operator").await?;
    assert!(
        matches!(confirm_outcome, RelationDecisionOutcome::NotFound),
        "confirming a nonexistent relation id must report NotFound"
    );

    let reject_outcome = handle.reject_relation(999_999_999, "operator").await?;
    assert!(
        matches!(reject_outcome, RelationDecisionOutcome::NotFound),
        "rejecting a nonexistent relation id must report NotFound"
    );

    Ok(())
}

#[tokio::test]
async fn confirm_relation_twice_returns_wrong_status_on_the_second_call() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let project_id = test_support::seed_project(
        &pool,
        &format!("lesson-relations-confirm-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let (older_id, newer_id) = seed_two_lessons(&handle, project_id).await?;

    let relation_id = handle
        .insert_lesson_relation(&draft(newer_id, older_id))
        .await?;

    let first = handle.confirm_relation(relation_id, "operator").await?;
    assert!(
        matches!(first, RelationDecisionOutcome::Decided(_)),
        "first confirm must succeed on a proposed relation"
    );

    let second = handle.confirm_relation(relation_id, "operator").await?;
    assert!(
        matches!(second, RelationDecisionOutcome::WrongStatus(ref s) if s == "confirmed"),
        "re-confirming an already-confirmed relation must report WrongStatus(\"confirmed\"), got {second:?}"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn confirmed_supersedes_excludes_only_the_older_lesson_from_injection() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let project_path = format!("lesson-relations-injection-{}", uuid::Uuid::new_v4());
    let project_id = test_support::seed_project(&pool, &project_path).await?;
    let (older_id, newer_id) = seed_two_lessons(&handle, project_id).await?;

    let before = handle.list_injectable_lessons(&project_path).await?;
    assert_eq!(
        before.len(),
        2,
        "both lessons must be injectable before any relation exists"
    );

    let relation_id = handle
        .insert_lesson_relation(&draft(newer_id, older_id))
        .await?;

    let while_proposed = handle.list_injectable_lessons(&project_path).await?;
    assert_eq!(
        while_proposed.len(),
        2,
        "a merely-proposed (not confirmed) relation must not exclude anything yet"
    );

    let outcome = handle.confirm_relation(relation_id, "operator").await?;
    assert!(matches!(outcome, RelationDecisionOutcome::Decided(_)));

    let after = handle.list_injectable_lessons(&project_path).await?;
    let after_ids: Vec<i64> = after.iter().map(|r| r.id).collect();
    assert!(
        !after_ids.contains(&older_id),
        "the superseded lesson must be excluded from injection once confirmed: {after_ids:?}"
    );
    assert!(
        after_ids.contains(&newer_id),
        "the superseding lesson must remain injectable: {after_ids:?}"
    );

    let (stage_code, archived_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT ls.code, l.archived_at
               FROM memory.lessons l
               JOIN memory.lesson_stages ls ON ls.id = l.stage_id
              WHERE l.id = $1",
        )
        .bind(older_id)
        .fetch_one(&pool)
        .await?;
    assert_ne!(
        stage_code, "archived",
        "confirming a supersedes relation must never archive the superseded lesson"
    );
    assert!(
        archived_at.is_none(),
        "confirming a supersedes relation must never set archived_at on the superseded lesson"
    );

    let still_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memory.lessons WHERE id = $1)")
            .bind(older_id)
            .fetch_one(&pool)
            .await?;
    assert!(
        still_exists,
        "confirming a supersedes relation must never delete the superseded lesson's row"
    );

    pool.close().await;
    Ok(())
}
