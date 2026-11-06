use chrono::{Duration, Utc};
use sqlx::PgPool;

use crate::turn_facts::{
    Fixture, NewTurn, Req, fixture, insert_request, insert_request_in, insert_response,
    insert_session, insert_turn_in, resolve, sample_row,
};

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

async fn judged_turn(
    tf: &Fixture,
    session_id: i64,
    request_id: i64,
    at_secs: i64,
    reaction: &str,
) -> anyhow::Result<()> {
    insert_request_in(
        tf,
        session_id,
        &Req::primary(request_id, at_secs, "ship it"),
    )
    .await?;
    let turn = insert_turn_in(
        tf,
        session_id,
        &NewTurn::closed(request_id, request_id, at_secs, at_secs + 5),
    )
    .await?;
    let mut row = sample_row(turn, session_id, Utc::now());
    resolve(&mut row);
    row.text_chars = 20;
    tf.handle.upsert_turn_facts(&[row]).await?;
    sqlx::query("UPDATE memory.operator_turns SET reaction = $2 WHERE id = $1")
        .bind(turn)
        .bind(reaction)
        .execute(tf.handle.pool())
        .await?;
    Ok(())
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
        .record_lesson_injections(97_001, &uuid, "primary", None, &[first])
        .await?;
    let written = tf
        .handle
        .record_mod_lesson_injections(97_002, &uuid, "primary", &[first, second, 9_999_999])
        .await?;
    assert_eq!(
        written, 2,
        "unknown lesson ids are skipped, known ones land"
    );

    let again = tf
        .handle
        .record_mod_lesson_injections(97_003, &uuid, "primary", &[first, second])
        .await?;
    assert_eq!(
        again, 0,
        "a later request carrying the same lessons adds nothing"
    );
    let later = tf
        .handle
        .record_mod_lesson_injections(97_003, &uuid, "primary", &[third])
        .await?;
    assert_eq!(later, 1);

    let rows = tf
        .handle
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
            .record_mod_lesson_injections(88_888_888, &uuid, "primary", &[lesson])
            .await
            .is_err(),
        "a request that was never captured breaks the foreign key and is reported"
    );
    assert_eq!(
        tf.handle
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
            .record_mod_lesson_injections(97_101, &uuid, "no-such-lane", &[lesson])
            .await?,
        0
    );
    assert_eq!(
        tf.handle
            .record_mod_lesson_injections(97_101, &uuid, "primary", &[])
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn the_comparison_counts_each_arm_and_ignores_sessions_outside_both() -> anyhow::Result<()> {
    let Some(tf) = fixture().await? else {
        return Ok(());
    };
    let pool = tf.handle.pool();
    let control_session = insert_session(pool, tf.project_id, Utc::now()).await?;
    let outside_session = insert_session(pool, tf.project_id, Utc::now()).await?;
    judged_turn(&tf, tf.session_id, 97_201, 0, "accepted").await?;
    judged_turn(&tf, tf.session_id, 97_202, 60, "corrected").await?;
    judged_turn(&tf, control_session, 97_203, 0, "accepted").await?;
    judged_turn(&tf, outside_session, 97_204, 0, "corrected").await?;
    insert_response(&tf, 97_201, "end_turn").await?;
    sqlx::query("UPDATE responses.responses SET token_usage = $1 WHERE request_id = 97201")
        .bind(serde_json::json!({
            "input_tokens": 100,
            "cache_read_input_tokens": 800,
            "cache_creation_input_tokens": 100
        }))
        .execute(pool)
        .await?;

    let mod_uuid = session_uuid(pool, tf.session_id).await?;
    let control_uuid = session_uuid(pool, control_session).await?;
    tf.handle
        .record_lesson_delivery(&mod_uuid, "mod", 2, 40)
        .await?;
    tf.handle
        .record_lesson_delivery(&control_uuid, "control", 0, 0)
        .await?;
    let first = seed_lesson(pool, tf.project_id, "run make fmt").await?;
    let second = seed_lesson(pool, tf.project_id, "name the toggle").await?;
    tf.handle
        .record_mod_lesson_injections(97_201, &mod_uuid, "primary", &[first, second])
        .await?;

    let rows = tf
        .handle
        .compare_lesson_delivery_arms(
            tf.base - Duration::hours(1),
            Utc::now() + Duration::hours(1),
        )
        .await?;
    let counts = |arm: &str| {
        rows.iter().find(|row| row.arm == arm).map(|row| {
            (
                row.sessions,
                row.operator_turns,
                row.judged_turns,
                row.corrected_turns,
                row.lessons_delivered,
                row.requests,
                row.usage_requests,
            )
        })
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(counts("mod"), Some((1, 2, 2, 1, 2, 2, 1)));
    assert_eq!(counts("control"), Some((1, 1, 1, 0, 0, 1, 0)));
    let moded = rows
        .iter()
        .find(|row| row.arm == "mod")
        .ok_or_else(|| anyhow::anyhow!("mod arm missing"))?;
    assert_eq!(
        (
            moded.input_tokens,
            moded.cache_read_tokens,
            moded.cache_write_tokens
        ),
        (100, 800, 100)
    );

    let later = tf
        .handle
        .compare_lesson_delivery_arms(
            Utc::now() + Duration::hours(2),
            Utc::now() + Duration::hours(3),
        )
        .await?;
    assert!(
        later
            .iter()
            .all(|row| row.sessions == 0 && row.operator_turns == 0),
        "an empty window still returns both arms, with zero counts"
    );
    assert_eq!(later.len(), 2);
    Ok(())
}
