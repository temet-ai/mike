use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::outcome_sync_support_tests::{Env, NewRecord, pair};

const PROPOSAL_SOURCE: &str = "operator_proposal_decision";
const ASKER_SOURCE: &str = "asker_followed_up";

fn guess(lesson_id: i64, accept: f64) -> (String, serde_json::Value) {
    (
        format!("proposal_{lesson_id}"),
        json!({
            "type": "choice", "choice": "accept", "confidence": accept,
            "probabilities": {"accept": accept, "reject": 1.0 - accept}
        }),
    )
}

async fn ordered_record(env: &Env, lesson_id: i64, accept: f64) -> anyhow::Result<i64> {
    let (key, answer) = guess(lesson_id, accept);
    let mut record = NewRecord::new("proposal_order", "ordered");
    record.composed = json!({"answers": {key: answer}, "verdict": {"score": 0.7}});
    env.record(record).await
}

async fn decide(env: &Env, lesson_id: i64, decision: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lesson_placement_reviews (lesson_id, decision, decided_at)
         VALUES ($1, $2, now() + interval '1 minute')",
    )
    .bind(lesson_id)
    .bind(decision)
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn proposal_order_guess_is_scored_by_the_operators_later_placement_decision()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let (liked, rejected, unliked, undecided) = (
        env.lesson().await?,
        env.lesson().await?,
        env.lesson().await?,
        env.lesson().await?,
    );
    let right = ordered_record(&env, liked, 0.8).await?;
    let wrong = ordered_record(&env, rejected, 0.8).await?;
    let right_decline = ordered_record(&env, unliked, 0.3).await?;
    let open = ordered_record(&env, undecided, 0.9).await?;
    decide(&env, liked, "accepted").await?;
    decide(&env, rejected, "declined").await?;
    decide(&env, unliked, "declined").await?;
    assert_eq!(env.sync().await?, 3);
    assert_eq!(
        env.labels(right).await?,
        vec![pair("right", PROPOSAL_SOURCE)]
    );
    assert_eq!(
        env.labels(wrong).await?,
        vec![pair("wrong", PROPOSAL_SOURCE)]
    );
    assert_eq!(
        env.labels(right_decline).await?,
        vec![pair("right", PROPOSAL_SOURCE)]
    );
    assert!(env.labels(open).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn proposal_order_call_time_label_is_not_counted_twice() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let lesson = env.lesson().await?;
    let record = ordered_record(&env, lesson, 0.8).await?;
    sqlx::query(
        "INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
         VALUES ($1, 'right', $2, '{}'::jsonb)",
    )
    .bind(record)
    .bind(PROPOSAL_SOURCE)
    .execute(env.handle.pool())
    .await?;
    decide(&env, lesson, "accepted").await?;
    assert_eq!(env.sync().await?, 0);
    assert_eq!(
        env.labels(record).await?,
        vec![pair("right", PROPOSAL_SOURCE)]
    );
    env.finish().await
}

struct Parties {
    asker: i64,
    replier: i64,
}

async fn other_session(env: &Env) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT id FROM sessions.sessions WHERE project_id = $1 AND id <> $2 ORDER BY id LIMIT 1",
    )
    .bind(env.project)
    .bind(env.session_id)
    .fetch_one(env.handle.pool())
    .await?)
}

async fn mail(
    env: &Env,
    from: i64,
    to: i64,
    in_reply_to: Option<i64>,
    at: DateTime<Utc>,
) -> anyhow::Result<(i64, Uuid)> {
    let uuid = Uuid::new_v4();
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO mailbox.messages
             (message_uuid, from_session_id, to_session_id, body, queued_at, in_reply_to_id)
         VALUES ($1, $2, $3, 'text', $4, $5)
         RETURNING id",
    )
    .bind(uuid)
    .bind(from)
    .bind(to)
    .bind(at)
    .bind(in_reply_to)
    .fetch_one(env.handle.pool())
    .await?;
    Ok((id, uuid))
}

struct Exchange {
    record: i64,
    reply_id: i64,
    reply_at: DateTime<Utc>,
}

async fn exchange(env: &Env, parties: &Parties, days_ago: i32) -> anyhow::Result<Exchange> {
    let now = Utc::now() - Duration::days(i64::from(days_ago));
    let (asked_id, asked_uuid) = mail(
        env,
        parties.asker,
        parties.replier,
        None,
        now - Duration::minutes(10),
    )
    .await?;
    let mut new = NewRecord::new("mail_reply_link", "chosen")
        .verdict(json!({"score": 0.9, "linked_message": asked_uuid}));
    new.age_days = days_ago;
    let record = env.record(new).await?;
    let reply_at = now + Duration::seconds(30);
    let (reply_id, _) = mail(
        env,
        parties.replier,
        parties.asker,
        Some(asked_id),
        reply_at,
    )
    .await?;
    Ok(Exchange {
        record,
        reply_id,
        reply_at,
    })
}

#[tokio::test]
async fn mail_reply_link_is_right_when_the_asker_writes_next_in_the_thread_and_wrong_on_a_re_ask()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let parties = Parties {
        asker: other_session(&env).await?,
        replier: env.session_id,
    };
    let followed = exchange(&env, &parties, 1).await?;
    let re_asked = exchange(&env, &parties, 2).await?;
    let quiet = exchange(&env, &parties, 3).await?;
    mail(
        &env,
        parties.asker,
        parties.replier,
        Some(followed.reply_id),
        followed.reply_at + Duration::hours(3),
    )
    .await?;
    mail(
        &env,
        parties.asker,
        parties.replier,
        None,
        re_asked.reply_at + Duration::minutes(20),
    )
    .await?;
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(followed.record).await?,
        vec![pair("right", ASKER_SOURCE)]
    );
    assert_eq!(
        env.labels(re_asked.record).await?,
        vec![pair("wrong", ASKER_SOURCE)]
    );
    assert!(env.labels(quiet.record).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}
