use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::eval_repo::EvalPersist as _;
use anyhow::Context;
use chrono::Utc;

use super::items::RunJudge;
use super::spend_guard::SpendLimits;
use super::{LabelOptions, RunOutcome, Runner, StartRequest};
use crate::analysis::judge::input_v2::INPUT_BUILDER_V2;
use crate::analysis::judge::rubric::{V2_2026_09_23, effective_rubric};
use crate::analysis::judge::slices::JudgeSlice;
use crate::judge_worker::v2_judge::V2JudgeConfig;
use crate::llm::{LlmRuntimeSettings, OpenRouterClient};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{self, TestDbHandle};
use crate::store::persistence::turn_facts::TurnFactsRow;

const VERDICT: &str = r#"{"reaction":"accepted","delivery":"delivered","confidence":0.9,"rationale":"the operator moved on","unmet_ask":"","evidence_quality":"verified"}"#;

fn facts(turn_id: i64, session_id: i64) -> TurnFactsRow {
    TurnFactsRow {
        operator_turn_id: turn_id,
        session_id,
        builder_version: "turn-facts-test".to_owned(),
        built_at: Utc::now(),
        max_request_id: 1,
        prompt_kind: "genuine".to_owned(),
        prompt_rule: None,
        prompt_kind_at_close: "genuine".to_owned(),
        prompt_rule_at_close: None,
        next_state: "none_session_settled".to_owned(),
        next_genuine_request_id: None,
        next_genuine_turn_id: None,
        next_genuine_at: None,
        next_state_at_close: "none_session_settled".to_owned(),
        next_genuine_request_id_at_close: None,
        skipped_before_next: serde_json::json!({}),
        reply_window_segment_ids: Vec::new(),
        reclassified_at: None,
        text_chars: 4,
        text_cut_at: 4,
        last_text_origin: None,
        text_spans: serde_json::json!([]),
        stop_request_id: None,
        stop_reason: None,
        aborted: false,
        interrupted: false,
        close_cause: "next_prompt".to_owned(),
        checks_run: serde_json::json!([]),
        checks_count: 0,
        checks_failed: 0,
        ended_on: "other".to_owned(),
        ended_on_version: "ended-on-v1".to_owned(),
        session_class: "operator".to_owned(),
        session_class_rule: "default".to_owned(),
    }
}

async fn seed(handle: &PersistHandle, n: i64) -> anyhow::Result<()> {
    let pool = handle.pool();
    let project_id = test_support::seed_project(pool, "jc10-runner").await?;
    let session = uuid::Uuid::new_v4().to_string();
    let spare = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(pool, &session, &spare, project_id).await?;
    test_support::seed_session_turns(pool, &session, project_id, n).await?;
    let turns: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT t.id, t.session_id FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session)
    .fetch_all(pool)
    .await?;
    let rows: Vec<TurnFactsRow> = turns.iter().map(|(t, s)| facts(*t, *s)).collect();
    handle.dossier().upsert_turn_facts(&rows).await?;
    sqlx::query(
        "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
         SELECT t.id, 'dev', 'hash' FROM memory.operator_turns t WHERE t.id = ANY($1)",
    )
    .bind(turns.iter().map(|(t, _)| *t).collect::<Vec<i64>>())
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis,
                                  label, tier, method, split, source)
         SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, 'reaction', 'accepted', 'gold',
                'operator_click', 'dev', 'spa'
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = ANY($1)",
    )
    .bind(turns.iter().map(|(t, _)| *t).collect::<Vec<i64>>())
    .execute(pool)
    .await?;
    Ok(())
}

async fn openrouter() -> anyhow::Result<(wiremock::MockServer, OpenRouterClient)> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "gen-test",
                "choices": [{"message": {"content": VERDICT}}],
                "usage": {"prompt_tokens": 100, "completion_tokens": 20, "cost": 0.002}
            })),
        )
        .mount(&server)
        .await;
    let llm = OpenRouterClient::new(
        "sk-test".to_owned(),
        "test-model".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Ok((server, llm))
}

fn judge() -> RunJudge {
    RunJudge::V2(V2JudgeConfig {
        rubric: effective_rubric(&V2_2026_09_23, None),
        builder: INPUT_BUILDER_V2,
        model: "test-model".to_owned(),
        llm_settings: LlmRuntimeSettings::default(),
        slices: Vec::new(),
    })
}

async fn request(handle: &PersistHandle) -> anyhow::Result<StartRequest> {
    let labels_as_of: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(handle.pool())
        .await?;
    Ok(StartRequest {
        bundle_id: None,
        judge: judge(),
        labels: LabelOptions {
            split: "dev".to_owned(),
            tiers: vec!["gold".to_owned()],
            methods: vec!["operator_click".to_owned()],
            holdout_batch: None,
            labels_as_of,
            limit: None,
        },
        purpose: "measure".to_owned(),
        repeat_of: None,
        max_usd: 1.0,
        expected_hash: None,
        dry_run: false,
    })
}

fn allow_every_judge_call() {
    let overlay = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
        crate::settings::SettingsOverlay {
            llm_ceiling_max_calls: 1000,
            llm_rate_limit_per_minute: 1000,
            ..crate::settings::SettingsOverlay::default()
        },
    ));
    crate::llm::limiter::install_runtime(overlay, None);
    crate::llm::limiter::reset_global_for_tests();
}

async fn db() -> anyhow::Result<Option<(TestDbHandle, PersistHandle)>> {
    allow_every_judge_call();
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((db, handle)))
}

#[tokio::test]
async fn eval_run_completes_with_one_item_per_label() -> anyhow::Result<()> {
    let Some((db, handle)) = db().await? else {
        return Ok(());
    };
    seed(&handle, 30).await?;
    let (_server, llm) = openrouter().await?;
    let limits = SpendLimits {
        ceiling_usd: 0.0,
        eval_allowance_usd: 0.0,
        run_max_usd: 10.0,
    };
    let runner = Runner {
        persist: &handle,
        llm: &llm,
        limits,
    };
    let outcome = runner.start(request(&handle).await?).await?;
    let RunOutcome::Finished { run_id, status, .. } = outcome else {
        anyhow::bail!("expected a finished run, got {outcome:?}");
    };
    assert_eq!(status, "complete");
    let items = handle.eval().all_eval_run_items(run_id).await?;
    assert_eq!(items.len(), 30);
    assert!(items.iter().all(|i| i.outcome == "judged"));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn eval_run_selected_bundle_keeps_its_id_in_run_and_verdict() -> anyhow::Result<()> {
    let Some((db, handle)) = db().await? else {
        return Ok(());
    };
    seed(&handle, 1).await?;
    let (server, llm) = openrouter().await?;
    let runner = Runner {
        persist: &handle,
        llm: &llm,
        limits: SpendLimits {
            ceiling_usd: 0.0,
            eval_allowance_usd: 0.0,
            run_max_usd: 1.0,
        },
    };
    let mut components = judge().components();
    components["model"] = serde_json::json!("selected-model");
    components["reasoning_effort"] = serde_json::json!("high");
    components["max_output_tokens"] = serde_json::json!(987);
    let bundle_id = handle
        .eval()
        .create_candidate_bundle("turn_judge", "", &components)
        .await?;
    let mut req = request(&handle).await?;
    req.bundle_id = Some(bundle_id);
    req.judge = super::bundle::load_bundle_judge(&handle, bundle_id).await?;
    let RunOutcome::Finished { run_id, status, .. } = runner.start(req).await? else {
        anyhow::bail!("expected a finished run");
    };
    assert_eq!(status, "complete");
    let run = handle
        .eval()
        .get_eval_run(run_id)
        .await?
        .context("missing run")?;
    assert_eq!(run.bundle_id, bundle_id);
    assert_eq!(run.model, "selected-model");
    let items = handle.eval().all_eval_run_items(run_id).await?;
    assert_eq!(items.len(), 1);
    let (verdicts, count) = handle
        .eval()
        .list_turn_verdicts_for_turn(items[0].operator_turn_id, None, 10, 0)
        .await?;
    assert_eq!(count, 1);
    assert_eq!(verdicts[0].bundle_id, Some(bundle_id));
    assert_eq!(verdicts[0].model, "selected-model");
    let requests = server
        .received_requests()
        .await
        .context("request capture")?;
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body)?;
    assert_eq!(body["model"], "selected-model");
    assert_eq!(body["max_tokens"], 987);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn eval_run_selected_bundle_rejects_different_judge_even_on_dry_run() -> anyhow::Result<()> {
    let Some((db, handle)) = db().await? else {
        return Ok(());
    };
    let (server, llm) = openrouter().await?;
    let runner = Runner {
        persist: &handle,
        llm: &llm,
        limits: SpendLimits {
            ceiling_usd: 0.0,
            eval_allowance_usd: 0.0,
            run_max_usd: 1.0,
        },
    };
    let mut components = judge().components();
    components["model"] = serde_json::json!("selected-model");
    let bundle_id = handle
        .eval()
        .create_candidate_bundle("turn_judge", "", &components)
        .await?;
    let mut req = request(&handle).await?;
    req.bundle_id = Some(bundle_id);
    req.dry_run = true;
    let result = runner.start(req).await;
    assert!(result.is_err_and(|error| error.to_string().contains("differs from selected bundle")));
    assert!(
        server
            .received_requests()
            .await
            .context("request capture")?
            .is_empty()
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn eval_run_pauses_at_its_budget_and_resume_completes_the_same_run() -> anyhow::Result<()> {
    let Some((db, handle)) = db().await? else {
        return Ok(());
    };
    seed(&handle, 30).await?;
    let (_server, llm) = openrouter().await?;
    let tight = SpendLimits {
        ceiling_usd: 0.0,
        eval_allowance_usd: 0.0,
        run_max_usd: 0.0100001,
    };
    let runner = Runner {
        persist: &handle,
        llm: &llm,
        limits: tight,
    };
    let mut req = request(&handle).await?;
    req.max_usd = 0.0100001;
    let RunOutcome::Finished { run_id, status, .. } = runner.start(req).await? else {
        anyhow::bail!("expected a paused run");
    };
    assert_eq!(status, "paused_budget");
    assert_eq!(handle.eval().all_eval_run_items(run_id).await?.len(), 5);
    sqlx::query("UPDATE eval.runs SET max_usd = 10 WHERE id = $1")
        .bind(run_id)
        .execute(handle.pool())
        .await?;
    let roomy = Runner {
        persist: &handle,
        llm: &llm,
        limits: SpendLimits {
            run_max_usd: 10.0,
            ..tight
        },
    };
    let RunOutcome::Finished { status, .. } = roomy.resume(run_id, &judge()).await? else {
        anyhow::bail!("expected the resumed run to finish");
    };
    assert_eq!(status, "complete");
    assert_eq!(handle.eval().all_eval_run_items(run_id).await?.len(), 30);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn eval_run_dry_run_makes_no_llm_call_and_writes_no_run() -> anyhow::Result<()> {
    let Some((db, handle)) = db().await? else {
        return Ok(());
    };
    seed(&handle, 4).await?;
    let (server, llm) = openrouter().await?;
    let limits = SpendLimits {
        ceiling_usd: 0.0,
        eval_allowance_usd: 0.0,
        run_max_usd: 1.0,
    };
    let runner = Runner {
        persist: &handle,
        llm: &llm,
        limits,
    };
    let mut req = request(&handle).await?;
    req.dry_run = true;
    let outcome = runner.start(req).await?;
    assert!(
        matches!(outcome, RunOutcome::DryRun { items: 4, .. }),
        "{outcome:?}"
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM eval.runs")
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(runs, 0);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn eval_run_with_slices_sends_them_and_records_them_on_run_and_verdict() -> anyhow::Result<()>
{
    let Some((db, handle)) = db().await? else {
        return Ok(());
    };
    seed(&handle, 1).await?;
    let (server, llm) = openrouter().await?;
    let runner = Runner {
        persist: &handle,
        llm: &llm,
        limits: SpendLimits {
            ceiling_usd: 0.0,
            eval_allowance_usd: 0.0,
            run_max_usd: 1.0,
        },
    };
    let mut req = request(&handle).await?;
    let RunJudge::V2(cfg) = &mut req.judge else {
        anyhow::bail!("the test judge is a v2 judge");
    };
    cfg.slices = vec![JudgeSlice::Turnfacts, JudgeSlice::WithholdNext];
    let RunOutcome::Finished { run_id, status, .. } = runner.start(req).await? else {
        anyhow::bail!("expected a finished run");
    };
    assert_eq!(status, "complete");
    let run = handle
        .eval()
        .get_eval_run(run_id)
        .await?
        .context("missing run")?;
    assert_eq!(run.slices, vec!["turnfacts", "withhold_next"]);
    let items = handle.eval().all_eval_run_items(run_id).await?;
    let (verdicts, _) = handle
        .eval()
        .list_turn_verdicts_for_turn(items[0].operator_turn_id, None, 10, 0)
        .await?;
    assert_eq!(verdicts[0].slices, vec!["turnfacts", "withhold_next"]);
    assert_eq!(verdicts[0].input_builder_version, "judge-input-v2.1+slices");
    let requests = server
        .received_requests()
        .await
        .context("request capture")?;
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body)?;
    let sent = body["messages"].to_string();
    assert!(sent.contains("FACTS ABOUT THIS TURN"));
    assert!(sent.contains("Interrupted (a request in the turn was aborted): no"));
    assert!(sent.contains("CONTEXT SECTIONS (background only)"));
    db.cleanup().await?;
    Ok(())
}
