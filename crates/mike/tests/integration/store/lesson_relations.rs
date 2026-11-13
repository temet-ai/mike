use mike::store::persistence::PersistHandle;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use mike::store::persistence::project_knowledge::KnowledgeQuery;
use mike::store::persistence::test_support;
use mike::store::persistence::{LessonRelationDraft, RelationDecisionOutcome};
use sqlx::postgres::PgPoolOptions;

use crate::test_db::TestDb;

async fn seed_two_lessons(handle: &PersistHandle, project_id: i64) -> anyhow::Result<(i64, i64)> {
    let older = handle
        .lessons().create_lesson_with_topic(NewLessonWrite {
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
        .lessons().create_lesson_with_topic(NewLessonWrite {
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
    let lessons_db = handle.lessons();

    let confirm_outcome = lessons_db.confirm_relation(999_999_999, "operator").await?;
    assert!(
        matches!(confirm_outcome, RelationDecisionOutcome::NotFound),
        "confirming a nonexistent relation id must report NotFound"
    );

    let reject_outcome = lessons_db.reject_relation(999_999_999, "operator").await?;
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
    let lessons_db = handle.lessons();
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

    let relation_id = lessons_db
        .insert_lesson_relation(&draft(newer_id, older_id))
        .await?;

    let first = lessons_db.confirm_relation(relation_id, "operator").await?;
    assert!(
        matches!(first, RelationDecisionOutcome::Decided(_)),
        "first confirm must succeed on a proposed relation"
    );

    let second = lessons_db.confirm_relation(relation_id, "operator").await?;
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
    let lessons_db = handle.lessons();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let project_path = format!("lesson-relations-injection-{}", uuid::Uuid::new_v4());
    let project_id = test_support::seed_project(&pool, &project_path).await?;
    let (older_id, newer_id) = seed_two_lessons(&handle, project_id).await?;

    let before = lessons_db.list_injectable_lessons(&project_path).await?;
    assert_eq!(
        before.len(),
        2,
        "both lessons must be injectable before any relation exists"
    );

    let relation_id = lessons_db
        .insert_lesson_relation(&draft(newer_id, older_id))
        .await?;

    let while_proposed = lessons_db.list_injectable_lessons(&project_path).await?;
    assert_eq!(
        while_proposed.len(),
        2,
        "a merely-proposed (not confirmed) relation must not exclude anything yet"
    );

    let outcome = lessons_db.confirm_relation(relation_id, "operator").await?;
    assert!(matches!(outcome, RelationDecisionOutcome::Decided(_)));

    let after = lessons_db.list_injectable_lessons(&project_path).await?;
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

async fn lesson(handle: &PersistHandle, project_id: i64, text: &str) -> anyhow::Result<i64> {
    Ok(handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "correction",
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

async fn supply_in_sessions(
    pool: &sqlx::PgPool,
    project_id: i64,
    lesson_id: i64,
    verdicts: &[&str],
) -> anyhow::Result<()> {
    for verdict in verdicts {
        let session = uuid::Uuid::new_v4().to_string();
        let request_id =
            test_support::seed_captured_request(pool, &session, project_id, "primary", b"{}")
                .await?;
        sqlx::query(
            "WITH item AS (
                 INSERT INTO memory.supplied_items
                        (session_id, request_id, item_kind, lesson_id, route, score, chance, item_text)
                 SELECT r.session_id, r.id, 'lesson', $2, 'proxy', 1.0, 1.0, l.text
                   FROM requests.requests r, memory.lessons l
                  WHERE r.id = $1 AND l.id = $2
                 RETURNING id)
             INSERT INTO memory.supplied_item_verdicts
                    (supplied_item_id, verdict, rationale, model, input_text)
             SELECT item.id, $3, 'seeded', 'fake-judge', '' FROM item",
        )
        .bind(request_id)
        .bind(lesson_id)
        .bind(*verdict)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn retired(
    pool: &sqlx::PgPool,
    lesson_id: i64,
) -> anyhow::Result<(Option<String>, Option<String>, Option<i64>)> {
    Ok(sqlx::query_as(
        "SELECT retired_kind, retired_reason, retired_by_lesson_id FROM memory.lessons WHERE id = $1",
    )
    .bind(lesson_id)
    .fetch_one(pool)
    .await?)
}

async fn found_in_search(
    handle: &PersistHandle,
    pool: &sqlx::PgPool,
    project_id: i64,
    query: &str,
    lesson_id: i64,
) -> anyhow::Result<Option<serde_json::Value>> {
    let project_uuid: uuid::Uuid =
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    let found = handle
        .knowledge()
        .search_project_knowledge(&KnowledgeQuery {
            project: project_uuid.to_string(),
            query: query.into(),
            kind: Some("lesson".into()),
            limit: Some(50),
            ..KnowledgeQuery::default()
        })
        .await?;
    let needle = format!("\"lesson_id\":{lesson_id}");
    Ok(found["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item.to_string().contains(&needle))
        .cloned())
}

#[tokio::test]
async fn retire_by_outcome_rarely_followed_lesson_leaves_supply_and_stays_searchable()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let lessons_db = handle.lessons();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let project_path = format!("lesson-outcome-{}", uuid::Uuid::new_v4());
    let project_id = test_support::seed_project(&pool, &project_path).await?;
    let ignored = lesson(
        &handle,
        project_id,
        "Quokkasort the fixture rows before diffing them.",
    )
    .await?;
    let followed = lesson(
        &handle,
        project_id,
        "Run the narrow wombat test before the suite.",
    )
    .await?;
    let unproven = lesson(
        &handle,
        project_id,
        "Numbat paths are relative to the checkout.",
    )
    .await?;
    let mut ignored_record = vec!["used"];
    ignored_record.extend(["ignored"; 9]);
    ignored_record.extend(["corrected_anyway"; 2]);
    supply_in_sessions(&pool, project_id, ignored, &ignored_record).await?;
    let mut followed_record = vec!["used"; 10];
    followed_record.extend(["ignored"; 2]);
    supply_in_sessions(&pool, project_id, followed, &followed_record).await?;
    supply_in_sessions(&pool, project_id, unproven, &["ignored"; 3]).await?;

    let outcome = lessons_db.lesson_outcome(ignored).await?;
    assert_eq!(
        (
            outcome.judged,
            outcome.used,
            outcome.corrected_anyway,
            outcome.ignored
        ),
        (12, 1, 2, 9),
        "the outcome record counts each judged session once by verdict"
    );

    let retired_now = lessons_db.retire_lessons_by_outcome(10, 0.15).await?;
    let first_reason = retired(&pool, ignored).await?.1;
    lessons_db.retire_lessons_by_outcome(10, 0.15).await?;
    let (kind, reason, by) = retired(&pool, ignored).await?;
    assert_eq!(
        reason, first_reason,
        "a second run leaves a retired lesson as it was"
    );
    assert_eq!(kind.as_deref(), Some("outcome"));
    assert!(by.is_none());
    let reason = reason.unwrap_or_default();
    assert!(reason.contains("1 of the 12"), "{reason}");
    assert!(reason.contains("ignored in 9"), "{reason}");
    assert_eq!(
        retired(&pool, followed).await?.0,
        None,
        "a followed lesson stays in supply"
    );
    assert_eq!(
        retired(&pool, unproven).await?.0,
        None,
        "three judged supplies are below the evidence minimum"
    );
    assert!(retired_now >= 1, "the run retires the ignored lesson");

    let supplied: Vec<i64> = lessons_db
        .list_injectable_lessons(&project_path)
        .await?
        .iter()
        .map(|row| row.id)
        .collect();
    assert!(!supplied.contains(&ignored), "{supplied:?}");
    assert!(
        supplied.contains(&followed) && supplied.contains(&unproven),
        "{supplied:?}"
    );

    let hit = found_in_search(&handle, &pool, project_id, "Quokkasort", ignored).await?;
    assert!(
        hit.is_some(),
        "a lesson retired by outcome is still found by search"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn adjudicated_contradiction_supersedes_only_the_older_lesson() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let lessons_db = handle.lessons();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let project_path = format!("lesson-contradiction-{}", uuid::Uuid::new_v4());
    let project_id = test_support::seed_project(&pool, &project_path).await?;
    let (older, newer) = seed_two_lessons(&handle, project_id).await?;
    let contested_old = lesson(&handle, project_id, "Platypus builds run on the Mac.").await?;
    let contested_new = lesson(&handle, project_id, "Platypus builds run on omit only.").await?;
    let agreed = LessonRelationDraft {
        kind: "contradicts",
        method: "adjudicated",
        adjudicator_agrees: Some(true),
        adjudicator_model: Some("fake-adjudicator"),
        adjudicator_rationale: Some("The newer lesson says the bug is fixed."),
        ..draft(newer, older)
    };
    let disputed = LessonRelationDraft {
        kind: "contradicts",
        method: "adjudicated",
        status: "contested",
        adjudicator_agrees: Some(false),
        ..draft(contested_new, contested_old)
    };
    let relation = lessons_db.insert_lesson_relation(&agreed).await?;
    lessons_db.insert_lesson_relation(&disputed).await?;

    let first = lessons_db.supersede_contradicted_lessons().await?;
    let second = lessons_db.supersede_contradicted_lessons().await?;
    assert!(first >= 1);
    assert_eq!(second, 0, "a second run retires nothing new");

    let (kind, reason, by) = retired(&pool, older).await?;
    assert_eq!(kind.as_deref(), Some("superseded"));
    assert_eq!(by, Some(newer));
    let reason = reason.unwrap_or_default();
    assert!(
        reason.contains(&format!("newer lesson {newer}")),
        "{reason}"
    );
    assert!(reason.contains("bug is fixed"), "{reason}");
    assert_eq!(retired(&pool, newer).await?.0, None);
    assert_eq!(
        retired(&pool, contested_old).await?.0,
        None,
        "a contested contradiction retires nothing"
    );

    let status: String = sqlx::query_scalar(
        "SELECT s.code FROM memory.lesson_relations r
           JOIN memory.lesson_relation_statuses s ON s.id = r.status_id WHERE r.id = $1",
    )
    .bind(relation)
    .fetch_one(&pool)
    .await?;
    assert_eq!(status, "confirmed");

    let supplied: Vec<i64> = lessons_db
        .list_injectable_lessons(&project_path)
        .await?
        .iter()
        .map(|row| row.id)
        .collect();
    assert!(
        !supplied.contains(&older) && supplied.contains(&newer),
        "{supplied:?}"
    );

    let hit = found_in_search(&handle, &pool, project_id, "mailbox delivery", older).await?;
    let Some(hit) = hit else {
        anyhow::bail!("the superseded lesson must still be found by search");
    };
    assert!(hit.to_string().contains("contradicted_by"), "{hit}");

    pool.close().await;
    Ok(())
}
