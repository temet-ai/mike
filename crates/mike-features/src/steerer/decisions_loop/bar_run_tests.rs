use chrono::{Duration, Utc};
use serde_json::json;

use super::bar_issues_tests::Fake;
use super::run_with;
use crate::store::persistence::PersistHandle;
use crate::store::test_support::TestDb;

const POINT: &str = "bar_run_point";

async fn seed(persist: &PersistHandle, right: usize, wrong: usize) -> anyhow::Result<()> {
    let right = i64::try_from(right)?;
    let total = right + i64::try_from(wrong)?;
    sqlx::query(
        "WITH made AS (
             INSERT INTO decisions.records
                 (point, mode, question_set, question_set_hash, model_requested, state,
                  questions, request_body, outcome, composed, applied)
             SELECT $1, 'on', 'test_v1', 'h', 'typesafe/jev-1.14', '{}'::jsonb, '{}'::jsonb,
                    '{}'::jsonb, 'ok', '{\"verdict\": {\"score\": 0.9}}'::jsonb, 'sent'
               FROM generate_series(1, $3)
             RETURNING id),
         numbered AS (
             SELECT id, row_number() OVER (ORDER BY id) - 1 AS i FROM made)
         INSERT INTO decisions.outcomes (record_id, label, label_source)
         SELECT id,
                CASE WHEN i < $2 THEN 'right' ELSE 'wrong' END,
                CASE WHEN i % 2 = 0 THEN 'guard' ELSE 'operator' END
           FROM numbered",
    )
    .bind(POINT)
    .bind(right)
    .bind(total)
    .execute(persist.pool())
    .await?;
    Ok(())
}

async fn settings_snapshot(persist: &PersistHandle) -> anyhow::Result<(String, String)> {
    let shared = sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(jsonb_agg(to_jsonb(s) ORDER BY key)::text, '[]') FROM config.settings s",
    )
    .fetch_one(persist.pool())
    .await?;
    let machine = sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(jsonb_agg(to_jsonb(s))::text, '[]') FROM config.machine_settings s",
    )
    .fetch_one(persist.pool())
    .await?;
    Ok((shared, machine))
}

#[tokio::test]
async fn a_run_with_a_point_under_the_bar_files_one_issue_and_writes_no_setting()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    seed(&persist, 12, 12).await?;
    let before = settings_snapshot(&persist).await?;
    let tracker = Fake::new(None, true);
    let now = Utc::now();

    let first = run_with(&persist, now, &tracker).await?;
    assert_eq!(first["bar_issues"][0]["point"], json!(POINT));
    assert_eq!(first["bar_issues"][0]["action"], json!("created"));
    assert_eq!(first["bar_issues"][0]["issue"], json!(501));
    let created = tracker
        .calls()
        .await
        .iter()
        .filter(|call| call.starts_with("create:"))
        .count();
    assert_eq!(created, 1);

    let calls_after_first = tracker.calls().await.len();
    let second = run_with(&persist, now + Duration::hours(1), &tracker).await?;
    assert_eq!(second["bar_issues"], json!([]));
    assert_eq!(tracker.calls().await.len(), calls_after_first);

    assert_eq!(settings_snapshot(&persist).await?, before);
    persist.pool().close().await;
    db.cleanup().await
}
