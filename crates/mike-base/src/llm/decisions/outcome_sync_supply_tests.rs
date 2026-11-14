use chrono::{Duration, Utc};
use serde_json::json;

use super::outcome_sync_support_tests::{Env, NewRecord, pair};

enum Item {
    Lesson(i64),
    Card { thread: i64, hash: &'static str },
}

async fn supplied(
    env: &Env,
    request_id: i64,
    item: Item,
    minutes_from_now: i64,
) -> anyhow::Result<i64> {
    let (kind, lesson, thread, hash) = match item {
        Item::Lesson(id) => ("lesson", Some(id), None, None),
        Item::Card { thread, hash } => ("card", None, Some(thread), Some(hash)),
    };
    Ok(sqlx::query_scalar!(
        "INSERT INTO memory.supplied_items
             (session_id, request_id, item_kind, lesson_id, thread_id, card_hash, route,
              item_text, supplied_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'proxy', 'text', $7)
         RETURNING id",
        env.session_id,
        request_id,
        kind,
        lesson,
        thread,
        hash,
        Utc::now() + Duration::minutes(minutes_from_now),
    )
    .fetch_one(env.handle.pool())
    .await?)
}

async fn verdict(env: &Env, item_id: i64, verdict: &str, hand: Option<&str>) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO memory.supplied_item_verdicts
             (supplied_item_id, verdict, rationale, model, input_text,
              hand_verdict, hand_checked_by, hand_checked_at)
         VALUES ($1, $2, 'r', 'm', 'i', $3::text, 'operator',
                 CASE WHEN $3::text IS NULL THEN NULL ELSE now() END)",
        item_id,
        verdict,
        hand,
    )
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

async fn thread(env: &Env, slug: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar!(
        "INSERT INTO work.threads (project_id, slug, title, status)
         VALUES ($1, $2, $2, 'active') RETURNING id",
        env.project,
        slug
    )
    .fetch_one(env.handle.pool())
    .await?)
}

#[tokio::test]
async fn lesson_supply_kept_lessons_are_scored_by_their_supplied_item_verdicts()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(2).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    let (used, ignored) = (env.lesson().await?, env.lesson().await?);
    let mut kept_used = NewRecord::new("lesson_supply", "sent")
        .verdict(json!({"score": 0.9, "keep": [used], "hold": []}));
    kept_used.request_id = Some(turns[0].start_request_id);
    let mut kept_ignored = NewRecord::new("lesson_supply", "sent")
        .verdict(json!({"score": 0.9, "keep": [ignored], "hold": []}));
    kept_ignored.request_id = Some(turns[1].start_request_id);
    let (a, b) = (
        env.record(kept_used).await?,
        env.record(kept_ignored).await?,
    );
    let item = supplied(&env, turns[0].start_request_id, Item::Lesson(used), 0).await?;
    verdict(&env, item, "used", None).await?;
    let item = supplied(&env, turns[1].start_request_id, Item::Lesson(ignored), 0).await?;
    verdict(&env, item, "ignored", None).await?;
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(a).await?,
        vec![pair("right", "sent_lesson_verdict")]
    );
    assert_eq!(
        env.labels(b).await?,
        vec![pair("wrong", "sent_lesson_verdict")]
    );
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn lesson_supply_held_lesson_that_later_proved_useful_was_a_wrong_hold() -> anyhow::Result<()>
{
    let Some(env) = Env::boot(2).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    let (useful, useless) = (env.lesson().await?, env.lesson().await?);
    let mut wrong_hold = NewRecord::new("lesson_supply", "held")
        .verdict(json!({"score": 0.7, "keep": [], "hold": [useful]}));
    wrong_hold.request_id = Some(turns[0].start_request_id);
    let mut right_hold = NewRecord::new("lesson_supply", "held")
        .verdict(json!({"score": 0.7, "keep": [], "hold": [useless]}));
    right_hold.request_id = Some(turns[1].start_request_id);
    let (a, b) = (env.record(wrong_hold).await?, env.record(right_hold).await?);
    let item = supplied(&env, turns[1].start_request_id, Item::Lesson(useful), 5).await?;
    verdict(&env, item, "used", None).await?;
    let item = supplied(&env, turns[1].start_request_id, Item::Lesson(useless), 5).await?;
    verdict(&env, item, "used", Some("ignored")).await?;
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(a).await?,
        vec![pair("wrong", "held_lesson_verdict")]
    );
    assert_eq!(
        env.labels(b).await?,
        vec![pair("right", "held_lesson_verdict")]
    );
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn thread_pick_is_scored_by_the_verdict_on_the_card_it_supplied() -> anyhow::Result<()> {
    let Some(env) = Env::boot(2).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    let (good, bad) = (thread(&env, "good").await?, thread(&env, "bad").await?);
    let mut ids = Vec::new();
    for (turn, thread_id, hash, said) in [
        (&turns[0], good, "aaa", "used"),
        (&turns[1], bad, "bbb", "ignored"),
    ] {
        let mut record = NewRecord::new("thread_pick", "chosen");
        record.session_id = Some(env.session_id);
        let record_id = env.record(record).await?;
        let signals = json!([{"slug": "x", "decision_record_id": record_id}]);
        sqlx::query!(
            "INSERT INTO memory.keeper_supplies
                 (session_uuid, thread_id, carrier, arm, resolution, signals)
             VALUES ($1, $2, 'proxy', 'keeper', 'one', $3)",
            env.session_uuid,
            thread_id,
            signals
        )
        .execute(env.handle.pool())
        .await?;
        let item = supplied(
            &env,
            turn.start_request_id,
            Item::Card {
                thread: thread_id,
                hash,
            },
            0,
        )
        .await?;
        verdict(&env, item, said, None).await?;
        ids.push(record_id);
    }
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(ids[0]).await?,
        vec![pair("right", "supplied_card_verdict")]
    );
    assert_eq!(
        env.labels(ids[1]).await?,
        vec![pair("wrong", "supplied_card_verdict")]
    );
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn supply_verdict_is_scored_against_the_operators_hand_verdict() -> anyhow::Result<()> {
    let Some(env) = Env::boot(1).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    let lesson = env.lesson().await?;
    let mut ids = Vec::new();
    for hand in [Some("used"), Some("ignored"), None] {
        let item = supplied(&env, turns[0].start_request_id, Item::Lesson(lesson), 0).await?;
        verdict(&env, item, "used", hand).await?;
        let record = NewRecord::new("supply_verdict", "labelled")
            .verdict(json!({"score": 0.8, "item_id": item, "verdict": "used"}));
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(ids[0]).await?,
        vec![pair("right", "hand_verdict")]
    );
    assert_eq!(
        env.labels(ids[1]).await?,
        vec![pair("wrong", "hand_verdict")]
    );
    assert!(env.labels(ids[2]).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn lesson_placement_is_scored_by_the_operators_placement_review() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let (accepted, declined, pending) = (
        env.lesson().await?,
        env.lesson().await?,
        env.lesson().await?,
    );
    let mut ids = Vec::new();
    for lesson in [accepted, declined, pending] {
        let record = NewRecord::new("lesson_relate", "chosen")
            .verdict(json!({"score": 0.8, "lesson_ids": [lesson]}));
        ids.push(env.record(record).await?);
    }
    for (lesson, decision) in [(accepted, "accepted"), (declined, "declined")] {
        sqlx::query!(
            "INSERT INTO memory.lesson_placement_reviews (lesson_id, decision, decided_at)
             VALUES ($1, $2, now() + interval '1 minute')",
            lesson,
            decision
        )
        .execute(env.handle.pool())
        .await?;
    }
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(ids[0]).await?,
        vec![pair("right", "placement_review")]
    );
    assert_eq!(
        env.labels(ids[1]).await?,
        vec![pair("wrong", "placement_review")]
    );
    assert!(env.labels(ids[2]).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn lesson_decisions_are_wrong_when_the_operator_rejected_the_lesson_and_right_once_it_survived()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let (rejected, kept, fresh) = (
        env.lesson().await?,
        env.lesson().await?,
        env.lesson().await?,
    );
    sqlx::query!(
        "UPDATE memory.lessons
            SET archived_at = now(),
                stage_id = (SELECT id FROM memory.lesson_stages WHERE code = 'archived'),
                archived_reason_id = (SELECT id FROM memory.lesson_archived_reasons
                                       WHERE code = 'rejected_by_operator')
          WHERE id = $1",
        rejected
    )
    .execute(env.handle.pool())
    .await?;
    let mut ids = Vec::new();
    for (point, lesson, age) in [
        ("lesson_shape", rejected, 1),
        ("topic_assign", kept, 8),
        ("topic_assign", fresh, 1),
    ] {
        let mut record =
            NewRecord::new(point, "chosen").verdict(json!({"score": 0.8, "lesson_ids": [lesson]}));
        record.age_days = age;
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 2);
    let source = "lesson_operator_verdict";
    assert_eq!(env.labels(ids[0]).await?, vec![pair("wrong", source)]);
    assert_eq!(env.labels(ids[1]).await?, vec![pair("right", source)]);
    assert!(env.labels(ids[2]).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}
