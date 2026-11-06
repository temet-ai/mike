use chrono::{DateTime, Utc};
use mike::store::persistence::PersistHandle;

use crate::test_db::TestDb;

#[tokio::test]
async fn record_judge_attempt_counts_attempts_and_resets_failures_on_success() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let session_uuid = uuid::Uuid::new_v4().to_string();
    handle.pin_session(&session_uuid).await?;
    let session_db_id = handle
        .resolve_session_db_id(&session_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session must exist after pin_session"))?;

    let judge_model = format!("m-{}", uuid::Uuid::new_v4());
    let t0 = Utc::now();
    let step = chrono::Duration::seconds(1);
    handle
        .record_judge_attempt(session_db_id, &judge_model, 1, t0, true)
        .await?;
    handle
        .record_judge_attempt(session_db_id, &judge_model, 1, t0 + step, true)
        .await?;
    let read = || async {
        sqlx::query_as::<_, (i32, i32, DateTime<Utc>)>(
            "SELECT attempt_count, consecutive_failures, last_attempt_at
               FROM session_judges.attempts
              WHERE session_id = $1 AND judge_model = $2 AND rubric_version = 1",
        )
        .bind(session_db_id)
        .bind(&judge_model)
        .fetch_one(handle.pool())
        .await
    };

    let (count, failures, _) = read().await?;
    assert_eq!((count, failures), (2, 2), "failures accumulate per attempt");

    handle
        .record_judge_attempt(session_db_id, &judge_model, 1, t0 + step + step, false)
        .await?;
    let (count, failures, last) = read().await?;
    assert_eq!(
        (count, failures),
        (3, 0),
        "a clean attempt resets the streak"
    );
    assert_eq!(
        last.timestamp_micros(),
        (t0 + step + step).timestamp_micros()
    );

    handle
        .record_judge_attempt(session_db_id, &judge_model, 1, t0, false)
        .await?;
    let (count, _, last) = read().await?;
    assert_eq!(count, 4);
    assert_eq!(
        last.timestamp_micros(),
        (t0 + step + step).timestamp_micros(),
        "an older attempt timestamp never moves last_attempt_at backwards"
    );

    Ok(())
}

#[tokio::test]
async fn repeated_digest_is_skipped_without_spending_an_attempt() -> anyhow::Result<()> {
    use mike::store::persistence::judge_digest::JudgeAttemptKey;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let session_uuid = uuid::Uuid::new_v4().to_string();
    handle.pin_session(&session_uuid).await?;
    let session_db_id = handle
        .resolve_session_db_id(&session_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session must exist after pin_session"))?;
    let judge_model = format!("m-{}", uuid::Uuid::new_v4());
    let key = JudgeAttemptKey {
        session_db_id,
        judge_model: &judge_model,
        rubric_version: 1,
    };
    let t0 = Utc::now();
    let later = t0 + chrono::Duration::minutes(10);

    assert!(
        !handle.skip_repeated_digest(&key, b"d1", t0).await?,
        "no attempt row yet, so nothing to repeat"
    );
    handle
        .record_judge_attempt(session_db_id, &judge_model, 1, t0, false)
        .await?;
    assert!(
        !handle.skip_repeated_digest(&key, b"d1", t0).await?,
        "no digest recorded yet"
    );
    handle.record_judged_digest(&key, b"d1").await?;
    assert!(!handle.skip_repeated_digest(&key, b"d2", later).await?);
    assert!(handle.skip_repeated_digest(&key, b"d1", later).await?);

    let (count, window, last) = sqlx::query_as::<_, (i32, i32, DateTime<Utc>)>(
        "SELECT attempt_count, window_attempts, last_attempt_at
           FROM session_judges.attempts
          WHERE session_id = $1 AND judge_model = $2 AND rubric_version = 1",
    )
    .bind(session_db_id)
    .bind(&judge_model)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!((count, window), (1, 1), "a skip spends no attempt");
    assert_eq!(
        last.timestamp_micros(),
        later.timestamp_micros(),
        "a skip moves last_attempt_at so selection waits for a newer turn"
    );
    Ok(())
}
