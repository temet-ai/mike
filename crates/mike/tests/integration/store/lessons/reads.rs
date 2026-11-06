use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::{LessonInsertRow, NewLessonWrite};
use mike::store::persistence::test_support;

use crate::store::support::{seed_agent, seed_daily_lesson_with_embedding, unit_vec};
use crate::test_db::TestDb;

#[tokio::test]
async fn find_similar_lessons_agent_scope_filters() -> anyhow::Result<()> {
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

    let project_id =
        test_support::seed_project(&pool, &format!("w3-agent-scope-{}", uuid::Uuid::new_v4()))
            .await?;
    let (agent_db_id, _agent_uuid) =
        seed_agent(&pool, &format!("w3-scope-agent-{}", uuid::Uuid::new_v4())).await?;
    let (other_agent_db_id, _other_agent_uuid) =
        seed_agent(&pool, &format!("w3-other-agent-{}", uuid::Uuid::new_v4())).await?;

    let scoped = seed_daily_lesson_with_embedding(
        &pool,
        project_id,
        Some(agent_db_id),
        "scoped to agent",
        &unit_vec(0),
    )
    .await?;
    let project_wide =
        seed_daily_lesson_with_embedding(&pool, project_id, None, "project wide", &unit_vec(0))
            .await?;
    let other_agent = seed_daily_lesson_with_embedding(
        &pool,
        project_id,
        Some(other_agent_db_id),
        "other agent",
        &unit_vec(0),
    )
    .await?;

    let probe = unit_vec(0);

    let scoped_hits = handle
        .find_similar_lessons(project_id, &probe, Some(agent_db_id), 10, 0_i64)
        .await?;
    let scoped_ids: Vec<i64> = scoped_hits.iter().map(|n| n.id).collect();
    assert!(
        scoped_ids.contains(&scoped),
        "agent-scoped query must include the agent's lesson"
    );
    assert!(
        scoped_ids.contains(&project_wide),
        "agent-scoped query must include project-wide (NULL-agent) lessons"
    );
    assert!(
        !scoped_ids.contains(&other_agent),
        "agent-scoped query must EXCLUDE another agent's lesson"
    );

    let all_hits = handle
        .find_similar_lessons(project_id, &probe, None, 10, 0_i64)
        .await?;
    let all_ids: Vec<i64> = all_hits.iter().map(|n| n.id).collect();
    assert!(all_ids.contains(&scoped));
    assert!(all_ids.contains(&project_wide));
    assert!(
        all_ids.contains(&other_agent),
        "None agent must widen to all agents"
    );

    Ok(())
}

async fn seed_four_topic_lessons(
    handle: &PersistHandle,
    project_id: i64,
) -> anyhow::Result<(LessonInsertRow, LessonInsertRow)> {
    let errors_a = handle
        .create_harvested_lesson(NewLessonWrite {
            project_id,
            kind: "convention",
            text: "always use anyhow for errors",
            topic: Some("errors"),
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("errors_a must insert (unique text)"))?;
    let errors_b = handle
        .create_harvested_lesson(NewLessonWrite {
            project_id,
            kind: "warning",
            text: "do not swallow errors silently",
            topic: Some("errors"),
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("errors_b must insert (unique text)"))?;
    handle
        .create_harvested_lesson(NewLessonWrite {
            project_id,
            kind: "preference",
            text: "prefer tokio::spawn for concurrency",
            topic: Some("concurrency"),
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    handle
        .create_harvested_lesson(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "no topic here",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    Ok((errors_a, errors_b))
}

#[tokio::test]
async fn list_lessons_by_topic_matches_exact_and_excludes_archived() -> anyhow::Result<()> {
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
        &format!("p13-list-by-topic-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let (errors_a, errors_b) = seed_four_topic_lessons(&handle, project_id).await?;

    let rows = handle.list_lessons_by_topic(project_id, "errors").await?;
    assert_eq!(
        rows.len(),
        2,
        "must return exactly the two 'errors' lessons"
    );
    for row in &rows {
        assert_eq!(
            row.topic.as_deref(),
            Some("errors"),
            "every returned row must carry topic='errors'"
        );
    }
    assert_eq!(
        rows[0].id, errors_b.id,
        "newest lesson must be first (ORDER BY created_at DESC)"
    );
    assert_eq!(rows[1].id, errors_a.id, "older lesson must be second");

    sqlx::query(
        "UPDATE memory.lessons l
            SET stage_id = archived_s.id,
                archived_at = NOW(),
                archived_reason_id = ar.id
           FROM memory.lesson_stages archived_s,
                memory.lesson_archived_reasons ar
          WHERE archived_s.code = 'archived'
            AND ar.code = 'trimmed'
            AND l.id = $1",
    )
    .bind(errors_b.id)
    .execute(&pool)
    .await?;

    let after_archive = handle.list_lessons_by_topic(project_id, "errors").await?;
    assert_eq!(
        after_archive.len(),
        1,
        "archived lesson must be excluded by the archived_at IS NULL predicate"
    );
    assert_eq!(
        after_archive[0].id, errors_a.id,
        "only the surviving active lesson must remain"
    );

    Ok(())
}

#[tokio::test]
async fn list_recall_lessons_limits_each_topic_newest_first() -> anyhow::Result<()> {
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

    let project_path = format!("recall-topics-{}", uuid::Uuid::new_v4());
    let project_id = test_support::seed_project(&pool, &project_path).await?;
    let (errors_a, errors_b) = seed_four_topic_lessons(&handle, project_id).await?;
    let topics = vec!["errors".to_owned(), "concurrency".to_owned()];

    let limited = handle
        .list_recall_lessons(&project_path, &topics, 1)
        .await?;
    let limited_topics: Vec<Option<&str>> = limited.iter().map(|r| r.topic.as_deref()).collect();
    assert_eq!(limited.len(), 2, "one lesson per requested topic");
    assert!(limited_topics.contains(&Some("concurrency")));
    let newest_error = limited
        .iter()
        .find(|r| r.topic.as_deref() == Some("errors"))
        .ok_or_else(|| anyhow::anyhow!("errors topic missing"))?;
    assert_eq!(
        newest_error.id, errors_b.id,
        "the newest lesson wins the limit"
    );

    let all = handle
        .list_recall_lessons(&project_path, &topics, 5)
        .await?;
    let error_ids: Vec<i64> = all
        .iter()
        .filter(|r| r.topic.as_deref() == Some("errors"))
        .map(|r| r.id)
        .collect();
    assert_eq!(error_ids, vec![errors_b.id, errors_a.id]);

    let unknown = handle
        .list_recall_lessons("recall-no-such-project", &topics, 5)
        .await?;
    assert!(unknown.is_empty(), "an unknown project path yields nothing");

    Ok(())
}
