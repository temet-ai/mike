use chrono::{Duration, Utc};
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use sqlx::PgPool;

use crate::turn_facts::{Req, fixture, insert_request};

async fn seed_lesson(pool: &PgPool, project_id: i64, text: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, $2, ls.id, src.id
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources src
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND src.code = 'operator'
         RETURNING id",
    )
    .bind(project_id)
    .bind(text)
    .fetch_one(pool)
    .await?)
}

async fn session_uuid(pool: &PgPool, session_id: i64) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT session_uuid::text FROM sessions.sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn mod_lessons_are_recorded_once_per_session_and_lesson_with_their_origin()
-> anyhow::Result<()> {
    let Some(tf) = fixture().await? else {
        return Ok(());
    };
    let pool = tf.handle.pool();
    insert_request(&tf, &Req::primary(97_001, 0, "first")).await?;
    insert_request(&tf, &Req::primary(97_002, 10, "second")).await?;
    insert_request(&tf, &Req::primary(97_003, 20, "third")).await?;
    let uuid = session_uuid(pool, tf.session_id).await?;
    let first = seed_lesson(pool, tf.project_id, "run make fmt").await?;
    let second = seed_lesson(pool, tf.project_id, "name the toggle").await?;
    let third = seed_lesson(pool, tf.project_id, "read the rule first").await?;

    tf.handle
        .lessons()
        .record_lesson_injections(97_001, &uuid, "primary", None, &[first])
        .await?;
    let written = tf
        .handle
        .lessons()
        .record_mod_lesson_injections(97_002, &uuid, "primary", &[first, second, 9_999_999])
        .await?;
    assert_eq!(
        written, 2,
        "unknown lesson ids are skipped, known ones land"
    );

    let again = tf
        .handle
        .lessons()
        .record_mod_lesson_injections(97_003, &uuid, "primary", &[first, second])
        .await?;
    assert_eq!(
        again, 0,
        "a later request carrying the same lessons adds nothing"
    );
    let later = tf
        .handle
        .lessons()
        .record_mod_lesson_injections(97_003, &uuid, "primary", &[third])
        .await?;
    assert_eq!(later, 1);

    let rows = tf
        .handle
        .lessons()
        .session_lessons(uuid.parse()?)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session not found"))?;
    let origins = |id: i64| {
        rows.injected
            .iter()
            .find(|row| row.lesson_id == id)
            .map(|row| row.origins.clone())
    };
    assert_eq!(
        origins(first),
        Some(vec!["mod".to_owned(), "proxy".to_owned()])
    );
    assert_eq!(origins(second), Some(vec!["mod".to_owned()]));
    assert_eq!(origins(third), Some(vec!["mod".to_owned()]));
    Ok(())
}

#[tokio::test]
async fn mod_lessons_for_an_unknown_request_session_lane_or_empty_list_are_handled()
-> anyhow::Result<()> {
    let Some(tf) = fixture().await? else {
        return Ok(());
    };
    let pool = tf.handle.pool();
    insert_request(&tf, &Req::primary(97_101, 0, "first")).await?;
    let uuid = session_uuid(pool, tf.session_id).await?;
    let lesson = seed_lesson(pool, tf.project_id, "run make fmt").await?;

    assert!(
        tf.handle
            .lessons()
            .record_mod_lesson_injections(88_888_888, &uuid, "primary", &[lesson])
            .await
            .is_err(),
        "a request that was never captured breaks the foreign key and is reported"
    );
    assert_eq!(
        tf.handle
            .lessons()
            .record_mod_lesson_injections(
                97_101,
                &uuid::Uuid::new_v4().to_string(),
                "primary",
                &[lesson]
            )
            .await?,
        0
    );
    assert_eq!(
        tf.handle
            .lessons()
            .record_mod_lesson_injections(97_101, &uuid, "no-such-lane", &[lesson])
            .await?,
        0
    );
    assert_eq!(
        tf.handle
            .lessons()
            .record_mod_lesson_injections(97_101, &uuid, "primary", &[])
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn holdings_survive_a_restart_and_a_refresh_releases_them() -> anyhow::Result<()> {
    let Some(tf) = fixture().await? else {
        return Ok(());
    };
    let pool = tf.handle.pool();
    let session = uuid::Uuid::new_v4().to_string();
    let lesson = seed_lesson(pool, tf.project_id, "hold this lesson").await?;
    let lessons = tf.handle.lessons();
    let since = Utc::now() - Duration::minutes(5);

    lessons.record_lesson_delivery(&session, 0, 0).await?;
    assert!(
        lessons
            .recent_client_sessions(since)
            .await?
            .contains(&session),
        "a client call marks the session even when it is given nothing"
    );

    let written = lessons
        .record_lesson_holdings(&session, "mod", &[(lesson, 1), (9_999_999, 1)])
        .await?;
    assert_eq!(written, 1, "unknown lesson ids are skipped");
    assert_eq!(
        lessons
            .record_lesson_holdings(&session, "proxy", &[(lesson, 1)])
            .await?,
        0,
        "the same revision again changes nothing"
    );
    assert_eq!(
        lessons
            .record_lesson_holdings(&session, "body", &[(lesson, 2)])
            .await?,
        1
    );
    let held: Vec<_> = lessons
        .recent_lesson_holdings(since)
        .await?
        .into_iter()
        .filter(|row| row.session == session)
        .map(|row| (row.lesson_id, row.revision))
        .collect();
    assert_eq!(held, vec![(lesson, 2)]);

    assert_eq!(lessons.forget_lesson_holdings(&session).await?, 1);
    assert!(
        lessons
            .recent_lesson_holdings(since)
            .await?
            .iter()
            .all(|row| row.session != session)
    );
    Ok(())
}
