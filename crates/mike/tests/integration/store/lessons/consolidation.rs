use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::test_support;

use crate::store::support::unit_vec;
use crate::test_db::TestDb;

async fn raw_lesson(
    handle: &PersistHandle,
    project_id: i64,
    text: &str,
) -> anyhow::Result<mike::store::persistence::lessons::LessonInsertRow> {
    Ok(handle
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
        .await?)
}

#[tokio::test]
async fn claim_promote_find_archive_consolidation_chain() -> anyhow::Result<()> {
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
        test_support::seed_project(&pool, &format!("w3-consolidation-{}", uuid::Uuid::new_v4()))
            .await?;
    let raw_a = raw_lesson(&handle, project_id, "first raw lesson").await?;
    let raw_b = raw_lesson(&handle, project_id, "second raw lesson").await?;

    let claimed = handle
        .claim_raw_lessons_for_consolidation(project_id, 50)
        .await?;
    assert_eq!(claimed.len(), 2, "both raw lessons must be claimable");
    assert!(
        claimed.iter().all(|l| l.stage == "raw"),
        "claimed lessons must all be raw"
    );

    handle
        .promote_lesson_to_daily(raw_a.id, &unit_vec(0), "bge-small-en-v1.5")
        .await?;
    let stage_a: String = sqlx::query_scalar(
        "SELECT ls.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.uuid = $1",
    )
    .bind(raw_a.uuid)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        stage_a, "daily",
        "promote_lesson_to_daily must move raw → daily"
    );

    let mut near = unit_vec(0);
    near[0] = 0.999;
    near[1] = 0.0447;
    let neighbours = handle
        .find_similar_lessons(project_id, &near, None, 5, raw_b.id)
        .await?;
    assert!(
        !neighbours.is_empty(),
        "the promoted daily lesson must be found"
    );
    assert_eq!(
        neighbours[0].id, raw_a.id,
        "nearest neighbour must be lesson A"
    );
    assert!(
        neighbours[0].distance < 0.15_f32,
        "near-identical vector must be within dedup distance: {}",
        neighbours[0].distance
    );

    handle
        .archive_lesson(raw_b.id, "duplicate", Some(raw_a.id))
        .await?;
    let (stage_b, dup_of): (String, Option<i64>) = sqlx::query_as(
        "SELECT ls.code, l.duplicate_of_id FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.uuid = $1",
    )
    .bind(raw_b.uuid)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        stage_b, "archived",
        "archive_lesson must set stage = archived"
    );
    let a_db_id: i64 = sqlx::query_scalar("SELECT id FROM memory.lessons WHERE uuid = $1")
        .bind(raw_a.uuid)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        dup_of,
        Some(a_db_id),
        "duplicate_of_id self-FK must resolve A's uuid to its bigint id"
    );

    Ok(())
}
