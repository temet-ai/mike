use chrono::{DateTime, Duration, Utc};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::web_repo::WebPersist as _;

use crate::test_db::TestDb;

async fn insert_call(
    handle: &PersistHandle,
    role: &str,
    model: &str,
    outcome: &str,
    http_status: Option<i32>,
    created_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO llm.calls
             (created_at, role, model, prompt_tokens, completion_tokens, reasoning_tokens,
              outcome, http_status)
         VALUES ($1, $2, $3, 100, 10, 0, $4, $5)",
    )
    .bind(created_at)
    .bind(role)
    .bind(model)
    .bind(outcome)
    .bind(http_status)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn role_outcomes_report_the_trailing_failure_streak_and_last_success() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let now = Utc::now();
    let model = "google/gemini-3.8-flash";
    insert_call(
        &handle,
        "judge_worker",
        model,
        "ok",
        Some(200),
        now - Duration::hours(30),
    )
    .await?;
    for minutes in [20, 10, 1] {
        insert_call(
            &handle,
            "judge_worker",
            model,
            "http_error",
            Some(402),
            now - Duration::minutes(minutes),
        )
        .await?;
    }
    insert_call(
        &handle,
        "session_judge",
        "codex/gpt-6-sol",
        "ok",
        None,
        now - Duration::hours(1),
    )
    .await?;
    insert_call(
        &handle,
        "eval",
        model,
        "http_error",
        Some(500),
        now - Duration::hours(1),
    )
    .await?;

    let outcomes = handle.web().llm_role_outcomes().await?;
    let roles: Vec<&str> = outcomes.iter().map(|o| o.role.as_str()).collect();
    assert_eq!(
        roles,
        ["judge_worker", "session_judge"],
        "eval is not a repeating worker"
    );

    let judge = outcomes
        .iter()
        .find(|o| o.role == "judge_worker")
        .ok_or_else(|| anyhow::anyhow!("judge_worker outcome must be present"))?;
    assert_eq!(judge.failed_streak, 3);
    assert_eq!(judge.last_outcome, "http_error");
    assert_eq!(judge.last_http_status, Some(402));
    assert_eq!(judge.model, model);
    assert!(
        judge
            .last_ok_at
            .is_some_and(|at| at < now - Duration::hours(29))
    );

    let session = outcomes
        .iter()
        .find(|o| o.role == "session_judge")
        .ok_or_else(|| anyhow::anyhow!("session_judge outcome must be present"))?;
    assert_eq!(session.failed_streak, 0);
    assert_eq!(session.last_outcome, "ok");

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn role_outcomes_ignore_calls_older_than_the_window() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    insert_call(
        &handle,
        "steerer",
        "openai/gpt-5.6-luna",
        "ok",
        Some(200),
        Utc::now() - Duration::days(9),
    )
    .await?;

    assert!(handle.web().llm_role_outcomes().await?.is_empty());

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn spend_rollups_count_codex_calls_as_subscription_calls() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let now = Utc::now();
    insert_call(&handle, "judge_worker", "codex/gpt-6-sol", "ok", None, now).await?;
    insert_call(&handle, "judge_worker", "codex/gpt-6-sol", "ok", None, now).await?;
    insert_call(
        &handle,
        "judge_worker",
        "openai/gpt-5.6-luna",
        "ok",
        Some(200),
        now,
    )
    .await?;

    let by_role = handle.llm_spend_by_role(None, None).await?;
    let judge = by_role
        .iter()
        .find(|r| r.key == "judge_worker")
        .ok_or_else(|| anyhow::anyhow!("judge_worker group must be present"))?;
    assert_eq!(judge.call_count, 3);
    assert_eq!(judge.subscription_call_count, 2);

    let by_model = handle.llm_spend_by_model(None, None).await?;
    let openrouter = by_model
        .iter()
        .find(|r| r.key == "openai/gpt-5.6-luna")
        .ok_or_else(|| anyhow::anyhow!("the OpenRouter model group must be present"))?;
    assert_eq!(openrouter.subscription_call_count, 0);

    let callers = handle.llm_spend_by_day_caller(None, None).await?;
    let caller = callers
        .iter()
        .find(|r| r.role == "judge_worker")
        .ok_or_else(|| anyhow::anyhow!("judge_worker caller row must be present"))?;
    assert_eq!(caller.subscription_call_count, 2);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn worker_toggle_states_return_only_the_requested_keys_with_their_age() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    handle.upsert_feature_toggle("judge_lessons", false).await?;
    handle
        .upsert_feature_toggle("weasel_intercept", false)
        .await?;

    let keys = vec!["judge_lessons".to_owned(), "steerer".to_owned()];
    let states = handle.web().worker_toggle_states(&keys).await?;
    let lessons = states
        .iter()
        .find(|s| s.key == "judge_lessons")
        .ok_or_else(|| anyhow::anyhow!("judge_lessons row must be present"))?;
    assert!(!lessons.enabled);
    assert!(Utc::now() - lessons.updated_at < Duration::minutes(5));
    assert!(states.iter().all(|s| s.key != "weasel_intercept"));

    assert_eq!(
        handle
            .web()
            .corrected_turns_since(Utc::now() - Duration::days(9))
            .await?,
        0
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
