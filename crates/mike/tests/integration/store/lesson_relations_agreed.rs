use mike::store::persistence::LessonRelationDraft;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use mike::store::persistence::test_support;
use sqlx::postgres::PgPoolOptions;

use crate::test_db::TestDb;

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

fn agreed(
    kind: &'static str,
    from_lesson_id: i64,
    to_lesson_id: i64,
) -> LessonRelationDraft<'static> {
    LessonRelationDraft {
        from_lesson_id,
        to_lesson_id,
        kind,
        method: "adjudicated",
        status: "proposed",
        candidate_distance: Some(0.15),
        evidence_json: serde_json::json!({}),
        proposer_model: Some("fake-proposer"),
        proposer_rationale: None,
        adjudicator_model: Some("fake-adjudicator"),
        adjudicator_agrees: Some(true),
        adjudicator_rationale: Some("The adjudicator agrees."),
        detector_version: mike::lesson_relations::DETECTOR_VERSION,
        decided_by: None,
    }
}

async fn retired_by(pool: &sqlx::PgPool, lesson_id: i64) -> anyhow::Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT retired_by_lesson_id FROM memory.lessons WHERE id = $1")
            .bind(lesson_id)
            .fetch_one(pool)
            .await?,
    )
}

async fn status(pool: &sqlx::PgPool, relation_id: i64) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "SELECT s.code FROM memory.lesson_relations r
           JOIN memory.lesson_relation_statuses s ON s.id = r.status_id WHERE r.id = $1",
    )
    .bind(relation_id)
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn agreed_supersedes_retires_the_superseded_lesson_and_agreed_refines_is_confirmed()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let project_id =
        test_support::seed_project(&pool, &format!("agreed-relations-{}", uuid::Uuid::new_v4()))
            .await?;
    let newer = lesson(
        &handle,
        project_id,
        "Builds deploy through mbx since November.",
    )
    .await?;
    let replaced = lesson(&handle, project_id, "Deploy builds never use mbx.").await?;
    let general = lesson(&handle, project_id, "Run the narrow check before pushing.").await?;
    let detail = lesson(
        &handle,
        project_id,
        "Run make test-t with the one filter before pushing.",
    )
    .await?;
    let lessons = handle.lessons();
    let supersedes = lessons
        .insert_lesson_relation(&agreed("supersedes", newer, replaced))
        .await?;
    let refines = lessons
        .insert_lesson_relation(&agreed("refines", detail, general))
        .await?;

    lessons.supersede_contradicted_lessons().await?;

    assert_eq!(retired_by(&pool, replaced).await?, Some(newer));
    assert_eq!(retired_by(&pool, newer).await?, None);
    assert_eq!(status(&pool, supersedes).await?, "confirmed");
    assert_eq!(status(&pool, refines).await?, "confirmed");
    assert_eq!(
        retired_by(&pool, general).await?,
        None,
        "refines retires nothing"
    );
    assert_eq!(
        retired_by(&pool, detail).await?,
        None,
        "refines retires nothing"
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn declared_supersedes_retires_the_old_lesson_and_an_older_detector_version_does_not()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let project_id = test_support::seed_project(
        &pool,
        &format!("declared-relations-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let old = lesson(&handle, project_id, "Cap builds at four jobs.").await?;
    let new = lesson(&handle, project_id, "Cap builds at half the cores.").await?;
    let kept = lesson(&handle, project_id, "Finish every part of the ask.").await?;
    let wrong = lesson(&handle, project_id, "Fix open issues yourself.").await?;
    let lessons = handle.lessons();
    lessons
        .insert_lesson_relation(&LessonRelationDraft {
            method: "declared",
            status: "confirmed",
            adjudicator_agrees: None,
            decided_by: Some("declared"),
            ..agreed("supersedes", new, old)
        })
        .await?;
    lessons
        .insert_lesson_relation(&LessonRelationDraft {
            detector_version: "an-older-detector",
            ..agreed("supersedes", wrong, kept)
        })
        .await?;

    lessons.supersede_contradicted_lessons().await?;

    assert_eq!(retired_by(&pool, old).await?, Some(new));
    assert_eq!(
        retired_by(&pool, kept).await?,
        None,
        "an adjudication from an older detector waits to be judged again"
    );
    pool.close().await;
    Ok(())
}
