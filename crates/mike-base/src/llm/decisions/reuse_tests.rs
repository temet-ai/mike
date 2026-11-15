use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use super::recent_answers;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

struct Row<'a> {
    set: &'a str,
    state: Value,
    outcome: &'a str,
    age_hours: i32,
    answers: Value,
}

fn answers(choice: &str) -> Value {
    json!({"unit_0": {"type": "choice", "choice": choice, "probabilities": {choice: 0.9}}})
}

async fn insert(persist: &PersistHandle, row: Row<'_>) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO decisions.records
             (created_at, point, mode, question_set, question_set_hash, model_requested,
              state, questions, request_body, outcome, answers)
         VALUES (now() - ($1 * interval '1 hour'), 'directive', 'on', $2, 'h', 'm',
                 $3, '{}'::jsonb, '{}'::jsonb, $4, $5)
         RETURNING id",
    )
    .bind(row.age_hours)
    .bind(row.set)
    .bind(row.state)
    .bind(row.outcome)
    .bind(row.answers)
    .fetch_one(persist.pool())
    .await?;
    Ok(id)
}

fn day() -> Duration {
    Duration::from_secs(24 * 3600)
}

#[tokio::test]
async fn recent_answers_returns_the_newest_row_with_the_same_state_and_set() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let set = format!("reuse_{}", Uuid::new_v4().simple());
    let state = json!({"units": {"unit_0": {"sentence": "Don't commit yet."}}});
    let older = Row {
        set: &set,
        state: state.clone(),
        outcome: "ok",
        age_hours: 5,
        answers: answers("neither"),
    };
    insert(&persist, older).await?;
    let newer = Row {
        set: &set,
        state: state.clone(),
        outcome: "ok",
        age_hours: 1,
        answers: answers("forbids"),
    };
    let newest = insert(&persist, newer).await?;
    let found = recent_answers(&persist, "directive", &set, &state, day()).await;
    let (id, asked) = found.ok_or_else(|| anyhow::anyhow!("no reuse"))?;
    assert_eq!(id, newest);
    assert_eq!(asked.choice("unit_0"), Some(("forbids", 0.9)));
    assert_eq!(asked.record_id, None);
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn recent_answers_skips_other_state_old_rows_and_failed_calls() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let set = format!("reuse_{}", Uuid::new_v4().simple());
    let state = json!({"units": {"unit_0": {"sentence": "Push when green."}}});
    let other = json!({"units": {"unit_0": {"sentence": "Push now."}}});
    let rows = [
        (other.clone(), "ok", 1),
        (state.clone(), "timeout", 1),
        (state.clone(), "ok", 48),
    ];
    for (row_state, outcome, age_hours) in rows {
        let row = Row {
            set: &set,
            state: row_state,
            outcome,
            age_hours,
            answers: answers("neither"),
        };
        insert(&persist, row).await?;
    }
    assert!(
        recent_answers(&persist, "directive", &set, &state, day())
            .await
            .is_none()
    );
    let week = Duration::from_secs(7 * 24 * 3600);
    assert!(
        recent_answers(&persist, "directive", &set, &state, week)
            .await
            .is_some()
    );
    assert!(
        recent_answers(&persist, "other_point", &set, &state, week)
            .await
            .is_none()
    );
    persist.pool().close().await;
    db.cleanup().await
}
