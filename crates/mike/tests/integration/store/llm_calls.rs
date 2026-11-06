use chrono::{DateTime, Utc};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::llm_calls::LlmCallInsert;

use crate::test_db::TestDb;

struct CallFixture {
    role: &'static str,
    steerer_id: Option<i64>,
    model: &'static str,
    cost_usd: f64,
    prompt_tokens: i32,
    completion_tokens: i32,
    outcome: &'static str,
    created_at: DateTime<Utc>,
}

async fn insert_call_at(handle: &PersistHandle, call: &CallFixture) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO llm.calls
             (created_at, role, model, cost_usd, prompt_tokens, completion_tokens,
              reasoning_tokens, outcome, steerer_id)
         VALUES ($1, $2, $3, $4, $5, $6, 0, $7, $8)",
    )
    .bind(call.created_at)
    .bind(call.role)
    .bind(call.model)
    .bind(call.cost_usd)
    .bind(call.prompt_tokens)
    .bind(call.completion_tokens)
    .bind(call.outcome)
    .bind(call.steerer_id)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn insert_minimal_call(
    persist: &PersistHandle,
    role: &str,
    cost_usd: Option<f64>,
) -> anyhow::Result<i64> {
    let id = persist
        .insert_llm_call(LlmCallInsert {
            role,
            model: "openai/gpt-5.6-luna",
            generation_id: Some("gen-abc123"),
            prompt_tokens: Some(120),
            completion_tokens: Some(40),
            reasoning_tokens: Some(0),
            cached_tokens: None,
            cost_usd,
            latency_ms: Some(842),
            outcome: "ok",
            http_status: Some(200),
            session_id: None,
            steerer_id: None,
            operator_turn_id: None,
            request_body: None,
        })
        .await?;
    Ok(id)
}

#[tokio::test]
async fn insert_and_sum_today_round_trips_and_is_zero_on_empty_table() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let empty_total = handle.sum_todays_llm_spend_usd().await?;
    assert_eq!(empty_total, 0.0);

    insert_minimal_call(&handle, "steerer", Some(0.03)).await?;
    insert_minimal_call(&handle, "judge_worker", Some(0.015)).await?;

    let total = handle.sum_todays_llm_spend_usd().await?;
    assert!((total - 0.045).abs() < 1e-9, "expected 0.045, got {total}");

    handle.pool().close().await;
    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn spend_rollups_group_correctly_and_respect_since_until_bounds() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    insert_minimal_call(&handle, "steerer", Some(0.02)).await?;
    insert_minimal_call(&handle, "steerer", Some(0.03)).await?;
    insert_minimal_call(&handle, "judge_worker", Some(0.05)).await?;

    let by_role = handle.llm_spend_by_role(None, None).await?;
    let steerer_row = by_role
        .iter()
        .find(|r| r.key == "steerer")
        .ok_or_else(|| anyhow::anyhow!("steerer group must be present"))?;
    assert_eq!(steerer_row.call_count, 2);
    assert!((steerer_row.cost_usd - 0.05).abs() < 1e-9);

    let judge_row = by_role
        .iter()
        .find(|r| r.key == "judge_worker")
        .ok_or_else(|| anyhow::anyhow!("judge_worker group must be present"))?;
    assert_eq!(judge_row.call_count, 1);

    let by_model = handle.llm_spend_by_model(None, None).await?;
    assert_eq!(by_model.len(), 1, "all three calls share one model id");
    assert_eq!(by_model[0].call_count, 3);

    let by_day = handle.llm_spend_by_day(None, None).await?;
    assert_eq!(by_day.len(), 1, "all three calls happen the same UTC day");
    assert_eq!(by_day[0].call_count, 3);

    let far_future = Utc::now() + chrono::Duration::days(365);
    let future_bounded = handle.llm_spend_by_role(Some(far_future), None).await?;
    assert!(
        future_bounded.is_empty(),
        "a since bound in the future must exclude every row"
    );

    let far_past = Utc::now() - chrono::Duration::days(365);
    let past_bounded = handle.llm_spend_by_role(Some(far_past), None).await?;
    assert_eq!(past_bounded.len(), 2, "steerer and judge_worker groups");

    handle.pool().close().await;
    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn insert_llm_call_round_trips_full_request_body_and_stores_null_when_absent()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let body = serde_json::json!({
        "model": "openai/gpt-5.6-luna",
        "messages": [
            {"role": "system", "content": "You are a session judge."},
            {
                "role": "user",
                "content": "RELATED SESSIONS\n\n- Fix the mailbox stranding bug: \
                             mailbox delivery must ack before the sender clears"
            }
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {"name": "session_verdict", "strict": true, "schema": {}}
        },
        "provider": {"require_parameters": true}
    });

    let id = handle
        .insert_llm_call(LlmCallInsert {
            role: "session_judge",
            model: "openai/gpt-5.6-luna",
            generation_id: Some("gen-full-body"),
            prompt_tokens: Some(500),
            completion_tokens: Some(120),
            reasoning_tokens: Some(0),
            cached_tokens: None,
            cost_usd: Some(0.01),
            latency_ms: Some(900),
            outcome: "ok",
            http_status: Some(200),
            session_id: None,
            steerer_id: None,
            operator_turn_id: None,
            request_body: Some(body.clone()),
        })
        .await?;

    let stored = sqlx::query_scalar!(
        r#"SELECT request_body AS "request_body: serde_json::Value" FROM llm.calls WHERE id = $1"#,
        id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        stored,
        Some(body),
        "request_body must round-trip byte-for-byte, not a sliced/summarized copy"
    );

    let no_body_id = insert_minimal_call(&handle, "steerer", Some(0.0)).await?;
    let stored_none = sqlx::query_scalar!(
        r#"SELECT request_body AS "request_body: serde_json::Value" FROM llm.calls WHERE id = $1"#,
        no_body_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(stored_none, None);

    handle.pool().close().await;
    handle_db.cleanup().await?;
    Ok(())
}

fn two_day_two_steerer_fixtures(
    today: DateTime<Utc>,
    yesterday: DateTime<Utc>,
) -> [CallFixture; 5] {
    [
        CallFixture {
            role: "steerer",
            steerer_id: Some(101),
            model: "model-a",
            cost_usd: 0.02,
            prompt_tokens: 100,
            completion_tokens: 10,
            outcome: "ok",
            created_at: yesterday,
        },
        CallFixture {
            role: "steerer",
            steerer_id: Some(101),
            model: "model-a",
            cost_usd: 0.03,
            prompt_tokens: 200,
            completion_tokens: 20,
            outcome: "http_error",
            created_at: yesterday,
        },
        CallFixture {
            role: "steerer",
            steerer_id: Some(202),
            model: "model-a",
            cost_usd: 0.01,
            prompt_tokens: 50,
            completion_tokens: 5,
            outcome: "ok",
            created_at: yesterday,
        },
        CallFixture {
            role: "judge_worker",
            steerer_id: None,
            model: "model-b",
            cost_usd: 0.05,
            prompt_tokens: 300,
            completion_tokens: 30,
            outcome: "ok",
            created_at: today,
        },
        CallFixture {
            role: "judge_worker",
            steerer_id: None,
            model: "model-b",
            cost_usd: 0.02,
            prompt_tokens: 150,
            completion_tokens: 15,
            outcome: "timeout",
            created_at: today,
        },
    ]
}

async fn assert_range_and_role_totals(
    handle: &PersistHandle,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> anyhow::Result<()> {
    let raw_total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(cost_usd), 0.0)::float8 FROM llm.calls
          WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(since)
    .bind(until)
    .fetch_one(handle.pool())
    .await?;

    let range_total = handle.sum_llm_spend_range(since, Some(until)).await?;
    assert!(
        (range_total - raw_total).abs() < 1e-9,
        "sum_llm_spend_range must match a raw aggregate over the same window: \
         rollup={range_total}, raw={raw_total}"
    );
    assert!((range_total - 0.13).abs() < 1e-9);

    let by_role = handle.llm_spend_by_role(Some(since), Some(until)).await?;
    let steerer_role = by_role
        .iter()
        .find(|r| r.key == "steerer")
        .ok_or_else(|| anyhow::anyhow!("steerer role must be present"))?;
    assert_eq!(steerer_role.call_count, 3);
    assert!((steerer_role.cost_usd - 0.06).abs() < 1e-9);
    let judge_role = by_role
        .iter()
        .find(|r| r.key == "judge_worker")
        .ok_or_else(|| anyhow::anyhow!("judge_worker role must be present"))?;
    assert_eq!(judge_role.call_count, 2);
    assert!((judge_role.cost_usd - 0.07).abs() < 1e-9);
    Ok(())
}

async fn assert_model_and_day_totals(
    handle: &PersistHandle,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> anyhow::Result<()> {
    let by_model = handle.llm_spend_by_model(Some(since), Some(until)).await?;
    let model_a = by_model
        .iter()
        .find(|r| r.key == "model-a")
        .ok_or_else(|| anyhow::anyhow!("model-a must be present"))?;
    assert_eq!(model_a.call_count, 3);
    let model_b = by_model
        .iter()
        .find(|r| r.key == "model-b")
        .ok_or_else(|| anyhow::anyhow!("model-b must be present"))?;
    assert_eq!(model_b.call_count, 2);

    let by_day = handle.llm_spend_by_day(Some(since), Some(until)).await?;
    assert_eq!(by_day.len(), 2, "two distinct UTC days must be present");
    let day_total: i64 = by_day.iter().map(|r| r.call_count).sum();
    assert_eq!(day_total, 5);
    Ok(())
}

async fn assert_day_caller_buckets(
    handle: &PersistHandle,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> anyhow::Result<()> {
    let by_day_caller = handle
        .llm_spend_by_day_caller(Some(since), Some(until))
        .await?;
    let day1_101 = by_day_caller
        .iter()
        .find(|r| r.steerer_id == Some(101))
        .ok_or_else(|| anyhow::anyhow!("steerer 101 bucket must be present"))?;
    assert_eq!(day1_101.call_count, 2);
    assert!((day1_101.cost_usd - 0.05).abs() < 1e-9);
    assert_eq!(
        day1_101.failed_count, 1,
        "the http_error call must count as failed"
    );

    let day1_202 = by_day_caller
        .iter()
        .find(|r| r.steerer_id == Some(202))
        .ok_or_else(|| anyhow::anyhow!("steerer 202 bucket must be present"))?;
    assert_eq!(day1_202.call_count, 1);
    assert_eq!(day1_202.failed_count, 0);

    let day2_judge = by_day_caller
        .iter()
        .find(|r| r.role == "judge_worker")
        .ok_or_else(|| anyhow::anyhow!("judge_worker bucket must be present"))?;
    assert_eq!(day2_judge.call_count, 2);
    assert_eq!(
        day2_judge.failed_count, 1,
        "the timeout call must count as failed"
    );
    Ok(())
}

#[tokio::test]
async fn rollup_backed_queries_match_a_raw_aggregate_across_two_days_and_two_steerers()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let today = Utc::now();
    let yesterday = today - chrono::Duration::days(1);

    for call in &two_day_two_steerer_fixtures(today, yesterday) {
        insert_call_at(&handle, call).await?;
    }

    let since = yesterday - chrono::Duration::hours(1);
    let until = today + chrono::Duration::hours(1);

    assert_range_and_role_totals(&handle, since, until).await?;
    assert_model_and_day_totals(&handle, since, until).await?;
    assert_day_caller_buckets(&handle, since, until).await?;

    handle.pool().close().await;
    handle_db.cleanup().await?;
    Ok(())
}
