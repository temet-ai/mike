use std::sync::Arc;

use axum::extract::{Query, State};

use crate::test_db::TestDb;

use mike::api::handlers::receipts_01::LlmSpendParams;
use mike::api::handlers::spend_01::get_llm_spend;
use mike::config::ProxyConfig;
use mike::settings::SettingsOverlay;
use mike::state::AppState;
use mike::store::persistence::llm_calls::LlmCallInsert;

fn empty_params() -> LlmSpendParams {
    LlmSpendParams {
        since: None,
        until: None,
    }
}

async fn persisted_test_state()
-> anyhow::Result<Option<(mike::store::test_support::TestDbHandle, AppState)>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire_isolated().await?;
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        "https://api.anthropic.com",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &db.database_url,
    ])?;
    let state = AppState::new(config).await?;
    Ok(Some((db, state)))
}

#[tokio::test]
async fn malformed_since_returns_bad_request_not_a_widened_window() -> anyhow::Result<()> {
    let Some((_db, state)) = persisted_test_state().await? else {
        return Ok(());
    };

    let params = LlmSpendParams {
        since: Some("not-a-real-timestamp".to_string()),
        until: None,
    };
    let Err(err) = get_llm_spend(State(state), Query(params)).await else {
        anyhow::bail!("a malformed `since` must be rejected, never silently ignored");
    };
    assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn no_params_defaults_to_the_seven_day_window() -> anyhow::Result<()> {
    let Some((_db, state)) = persisted_test_state().await? else {
        return Ok(());
    };

    let response = get_llm_spend(State(state), Query(empty_params()))
        .await
        .map_err(|e| anyhow::anyhow!("handler returned an error response: {e:?}"))?
        .0;

    assert!(response.window.defaulted);
    assert!(response.window.since.is_some());
    assert!(response.window.until.is_some());
    Ok(())
}

async fn insert_steerer_and_judge_worker_calls(
    handle: &mike::store::persistence::PersistHandle,
) -> anyhow::Result<()> {
    handle
        .insert_llm_call(LlmCallInsert {
            role: "steerer",
            model: "deepseek/deepseek-v4.1-flash",
            generation_id: Some("gen-1"),
            prompt_tokens: Some(1_200),
            completion_tokens: Some(5_700),
            reasoning_tokens: Some(5_000),
            cached_tokens: None,
            cost_usd: Some(0.06),
            latency_ms: Some(1_500),
            outcome: "ok",
            http_status: Some(200),
            session_id: None,
            steerer_id: None,
            operator_turn_id: None,
            request_body: None,
            response_body: None,
        })
        .await?;
    handle
        .insert_llm_call(LlmCallInsert {
            role: "judge_worker",
            model: "openai/gpt-5.6-luna",
            generation_id: Some("gen-2"),
            prompt_tokens: Some(800),
            completion_tokens: Some(200),
            reasoning_tokens: Some(0),
            cached_tokens: None,
            cost_usd: Some(0.02),
            latency_ms: Some(400),
            outcome: "ok",
            http_status: Some(200),
            session_id: None,
            steerer_id: None,
            operator_turn_id: None,
            request_body: None,
            response_body: None,
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn reports_today_totals_split_by_model_and_role_against_the_ceiling() -> anyhow::Result<()> {
    let Some((_db, state)) = persisted_test_state().await? else {
        return Ok(());
    };
    let handle = state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence was just configured above"))?;
    insert_steerer_and_judge_worker_calls(handle).await?;

    let overlay = SettingsOverlay {
        llm_daily_spend_ceiling_usd: 0.05,
        llm_spend_alert_fraction: 0.5,
        ..SettingsOverlay::default()
    };
    state.settings_overlay.store(Arc::new(overlay));

    let params = LlmSpendParams {
        since: Some((chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339()),
        until: Some((chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339()),
    };
    let response = get_llm_spend(State(state.clone()), Query(params))
        .await
        .map_err(|e| anyhow::anyhow!("handler returned an error response: {e:?}"))?
        .0;

    assert!(
        (response.today.spent_usd - 0.08).abs() < 1e-9,
        "expected today.spent_usd == 0.08, got {}",
        response.today.spent_usd
    );
    assert_eq!(response.today.ceiling_usd, 0.05);
    assert!(response.today.ceiling_enabled);
    assert_eq!(response.today.alert_threshold_usd, Some(0.025));
    assert!(response.today.over_alert_threshold);
    assert!(response.today.over_ceiling);

    assert_eq!(response.totals.call_count, 2);
    assert_eq!(response.totals.reasoning_tokens, 5_000);

    let steerer_row = response
        .by_role
        .iter()
        .find(|r| r.key == "steerer")
        .ok_or_else(|| anyhow::anyhow!("steerer role must be present in by_role"))?;
    assert_eq!(steerer_row.call_count, 1);
    assert_eq!(steerer_row.reasoning_tokens, 5_000);

    let deepseek_row = response
        .by_model
        .iter()
        .find(|r| r.key == "deepseek/deepseek-v4.1-flash")
        .ok_or_else(|| anyhow::anyhow!("deepseek model must be present in by_model"))?;
    assert!((deepseek_row.cost_usd - 0.06).abs() < 1e-9);

    let far_past_params = LlmSpendParams {
        since: Some((chrono::Utc::now() - chrono::Duration::days(400)).to_rfc3339()),
        until: Some((chrono::Utc::now() - chrono::Duration::days(399)).to_rfc3339()),
    };
    let narrowed = get_llm_spend(State(state.clone()), Query(far_past_params))
        .await
        .map_err(|e| anyhow::anyhow!("handler returned an error response: {e:?}"))?
        .0;
    assert!(
        (narrowed.today.spent_usd - 0.08).abs() < 1e-9,
        "today's total must not change when the rollup window excludes today"
    );
    assert!(
        narrowed.by_day.is_empty(),
        "a window with no matching rows must produce empty rollups, not an error"
    );

    Ok(())
}

async fn steerer_id_by_name(
    handle: &mike::store::persistence::PersistHandle,
    name: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar("SELECT id FROM steering.steerers WHERE name = $1")
        .bind(name)
        .fetch_one(handle.pool())
        .await?;
    Ok(id)
}

async fn insert_two_steerer_calls_and_a_judge_call(
    handle: &mike::store::persistence::PersistHandle,
) -> anyhow::Result<()> {
    let verifier_id = steerer_id_by_name(handle, "verifier").await?;
    let refocus_id = steerer_id_by_name(handle, "refocus").await?;

    handle
        .insert_llm_call(LlmCallInsert {
            role: "steerer",
            model: "deepseek/deepseek-v4.1-flash",
            generation_id: Some("gen-verifier-1"),
            prompt_tokens: Some(1_000),
            completion_tokens: Some(200),
            reasoning_tokens: Some(0),
            cached_tokens: None,
            cost_usd: Some(0.04),
            latency_ms: Some(900),
            outcome: "ok",
            http_status: Some(200),
            session_id: None,
            steerer_id: Some(verifier_id),
            operator_turn_id: None,
            request_body: None,
            response_body: None,
        })
        .await?;
    handle
        .insert_llm_call(LlmCallInsert {
            role: "steerer",
            model: "deepseek/deepseek-v4.1-flash",
            generation_id: Some("gen-verifier-2"),
            prompt_tokens: Some(2_000),
            completion_tokens: Some(300),
            reasoning_tokens: Some(0),
            cached_tokens: None,
            cost_usd: Some(0.06),
            latency_ms: Some(1_100),
            outcome: "http_error",
            http_status: Some(500),
            session_id: None,
            steerer_id: Some(verifier_id),
            operator_turn_id: None,
            request_body: None,
            response_body: None,
        })
        .await?;
    handle
        .insert_llm_call(LlmCallInsert {
            role: "steerer",
            model: "deepseek/deepseek-v4.1-flash",
            generation_id: Some("gen-refocus-1"),
            prompt_tokens: Some(500),
            completion_tokens: Some(100),
            reasoning_tokens: Some(0),
            cached_tokens: None,
            cost_usd: Some(0.01),
            latency_ms: Some(400),
            outcome: "ok",
            http_status: Some(200),
            session_id: None,
            steerer_id: Some(refocus_id),
            operator_turn_id: None,
            request_body: None,
            response_body: None,
        })
        .await?;
    handle
        .insert_llm_call(LlmCallInsert {
            role: "judge_worker",
            model: "openai/gpt-5.6-luna",
            generation_id: Some("gen-judge-1"),
            prompt_tokens: Some(800),
            completion_tokens: Some(150),
            reasoning_tokens: Some(0),
            cached_tokens: None,
            cost_usd: Some(0.02),
            latency_ms: Some(500),
            outcome: "ok",
            http_status: Some(200),
            session_id: None,
            steerer_id: None,
            operator_turn_id: None,
            request_body: None,
            response_body: None,
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn by_caller_and_by_day_caller_split_steerers_from_each_other_and_from_the_judge()
-> anyhow::Result<()> {
    let Some((_db, state)) = persisted_test_state().await? else {
        return Ok(());
    };
    let handle = state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence was just configured above"))?;
    insert_two_steerer_calls_and_a_judge_call(handle).await?;

    let params = LlmSpendParams {
        since: Some((chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339()),
        until: Some((chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339()),
    };
    let response = get_llm_spend(State(state.clone()), Query(params))
        .await
        .map_err(|e| anyhow::anyhow!("handler returned an error response: {e:?}"))?
        .0;

    let verifier = response
        .by_caller
        .iter()
        .find(|c| c.key == "steerer:verifier")
        .ok_or_else(|| anyhow::anyhow!("steerer:verifier must be present in by_caller"))?;
    assert_eq!(verifier.label, "Steerer \u{b7} Verifier");
    assert_eq!(verifier.call_count, 2);
    assert!((verifier.cost_usd - 0.10).abs() < 1e-9);
    assert!((verifier.avg_cost_per_call - 0.05).abs() < 1e-9);
    assert!((verifier.avg_prompt_tokens - 1_500.0).abs() < 1e-9);
    assert_eq!(
        verifier.failed_count, 1,
        "the http_error call must count as failed"
    );

    let refocus = response
        .by_caller
        .iter()
        .find(|c| c.key == "steerer:refocus")
        .ok_or_else(|| anyhow::anyhow!("steerer:refocus must be present in by_caller"))?;
    assert_eq!(refocus.label, "Steerer \u{b7} Refocus");
    assert_eq!(refocus.call_count, 1);
    assert_eq!(refocus.failed_count, 0);

    let judge = response
        .by_caller
        .iter()
        .find(|c| c.key == "judge_worker")
        .ok_or_else(|| anyhow::anyhow!("judge_worker must be present in by_caller"))?;
    assert_eq!(judge.label, "Turn judge");
    assert_eq!(judge.call_count, 1);
    assert!(judge.steerer_id.is_none());

    assert!(
        response.by_caller[0].cost_usd >= response.by_caller[1].cost_usd,
        "by_caller must be sorted cost_usd desc"
    );

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let day_keys: std::collections::HashSet<&str> = response
        .by_day_caller
        .iter()
        .filter(|r| r.day == today)
        .map(|r| r.key.as_str())
        .collect();
    assert!(day_keys.contains("steerer:verifier"));
    assert!(day_keys.contains("steerer:refocus"));
    assert!(day_keys.contains("judge_worker"));

    Ok(())
}
