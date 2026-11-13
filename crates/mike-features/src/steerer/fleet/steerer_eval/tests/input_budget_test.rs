use std::sync::Arc;

use tokio::sync::RwLock;

use super::super::*;
use super::support::{
    evaluate_turn, make_fleet_ctx, make_verifier_steerer, mock_openrouter_returning,
    seed_project_and_session,
};
use crate::settings::SettingsOverlay;
use crate::steerer::arming::has_explicit_done_claim;
use crate::steerer::fleet::input_budget::VerifierInputBudget;
use crate::steerer::fleet::verifier_judge::build_verifier_judge_context;
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::retention::RetentionPolicy;

const TOOL_USES: i64 = 350;
const NO_FIRE: &str = r#"{"fires":false,"confidence":0.9,"reason":"ok","nudge":""}"#;

async fn seed_request(
    pool: &sqlx::PgPool,
    session_db_id: i64,
    project_id: i64,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
                session_id, project_id, method_id, path, is_streaming,
                message_count, is_tool_loop, status_id,
                params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
                created_at, completed_at
             )
             SELECT $1, $2, rm.id, '/v1/messages', false,
                    0, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, NOW(), NOW()
               FROM requests.methods rm, requests.statuses rs,
                    requests.turn_types rtt, requests.turn_origins rto
              WHERE rm.code = 'POST' AND rs.code = 'complete'
                AND rtt.code = 'user_prompt' AND rto.code = 'primary'
             RETURNING id",
    )
    .bind(session_db_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, session_id, ord, tool_name, input)
         SELECT 'toolu_budget_' || g, $1, $2, g, 'Bash',
                jsonb_build_object('command', 'cmd ' || g)
           FROM generate_series(1, $3::int) g",
    )
    .bind(id)
    .bind(session_db_id)
    .bind(TOOL_USES)
    .execute(pool)
    .await?;
    Ok(id)
}

fn overlay_with_budget(code: &str) -> Arc<SettingsOverlay> {
    Arc::new(SettingsOverlay {
        steerer_verifier_input_budget: code.to_string(),
        ..SettingsOverlay::default()
    })
}

fn shown_bash_rows(list: &str) -> usize {
    list.lines().filter(|l| l.contains(". Bash — cmd ")).count()
}

#[tokio::test]
async fn context_builder_under_v1_issues_recent_and_counts_not_full_list() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (project_id, session_uuid, session_db_id) =
        seed_project_and_session(&pool, "proj-budget-v1").await?;
    seed_request(&pool, session_db_id, project_id).await?;

    let ctx = build_verifier_judge_context(
        &handle,
        session_uuid,
        Some(session_db_id),
        None,
        VerifierInputBudget::V1,
    )
    .await;
    let first = ctx.session_action_list.lines().next().unwrap_or_default();
    assert!(first.contains(" earlier calls not shown: "), "{first}");
    let omitted: usize = first
        .trim_start_matches('(')
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let shown = shown_bash_rows(&ctx.session_action_list);
    assert!(shown <= 200, "never the full list: {shown} rows shown");
    assert_eq!(omitted + shown, TOOL_USES as usize);
    assert!(
        first.contains(&format!("{omitted} commands of which 0 failed"))
            || first.contains("commands of which")
    );
    assert!(ctx.session_action_list.contains("350. Bash — cmd 350"));
    assert_eq!(ctx.budget, VerifierInputBudget::V1);

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn context_builder_uncapped_issues_full_list() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (project_id, session_uuid, session_db_id) =
        seed_project_and_session(&pool, "proj-budget-uncapped").await?;
    seed_request(&pool, session_db_id, project_id).await?;

    let ctx = build_verifier_judge_context(
        &handle,
        session_uuid,
        Some(session_db_id),
        None,
        VerifierInputBudget::Uncapped,
    )
    .await;
    assert_eq!(
        shown_bash_rows(&ctx.session_action_list),
        TOOL_USES as usize
    );
    assert!(!ctx.session_action_list.contains("earlier calls not shown"));

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

async fn last_user_message_text(mock: &wiremock::MockServer) -> String {
    let requests = mock.received_requests().await.unwrap_or_default();
    requests
        .last()
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .unwrap_or_default()
}

#[tokio::test]
async fn setting_switch_changes_the_next_evaluation_without_restart() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (project_id, session_uuid, session_db_id) =
        seed_project_and_session(&pool, "proj-budget-switch").await?;
    let request_id = seed_request(&pool, session_db_id, project_id).await?;
    let (mock, llm) = mock_openrouter_returning(NO_FIRE).await?;
    let steerer = make_verifier_steerer();
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let ctx = make_fleet_ctx(store, Some(handle), llm);
    let eval = || EvalInputs {
        eval_text: "All tests pass, this is complete.".to_string(),
        operator_ask: "finish".to_string(),
        last_user_message: "finish".to_string(),
        trigger_point: "response",
        ..Default::default()
    };

    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        Some(request_id),
        eval(),
    )
    .await;
    let capped = last_user_message_text(&mock).await;
    assert!(capped.contains("earlier calls not shown"));
    assert!(capped.contains("older calls are counted in the first line"));

    ctx.settings.store(overlay_with_budget("uncapped"));
    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        Some(request_id),
        eval(),
    )
    .await;
    let full = last_user_message_text(&mock).await;
    assert!(!full.contains("earlier calls not shown"));
    assert!(full.contains("every tool call this session has made"));
    assert!(full.contains("350. Bash — cmd 350"));
    assert!(full.contains("1. Bash — cmd 1 → no result"));

    ctx.settings.store(overlay_with_budget("verifier-input-v1"));
    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        Some(request_id),
        eval(),
    )
    .await;
    let again = last_user_message_text(&mock).await;
    assert!(again.contains("earlier calls not shown"));
    assert!(again.chars().count() < full.chars().count());

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

fn long_text_with_early_done_claim() -> String {
    format!("All done, finished.{}", " filler".repeat(3_000))
}

#[test]
fn pattern_gate_reads_whole_eval_text_under_v1() -> anyhow::Result<()> {
    let text = long_text_with_early_done_claim();
    let mut steerer = make_verifier_steerer();
    steerer.invocation = "pattern".to_string();
    steerer.pattern = Some("all done".to_string());
    let ctx = gate_ctx()?;
    let eval_inputs = EvalInputs {
        eval_text: text,
        operator_ask: "finish".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    assert_eq!(
        VerifierInputBudget::for_steerer_kind(&ctx.settings, "verifier"),
        VerifierInputBudget::V1
    );
    assert!(invocation_gate_passed(&ctx, &steerer, &eval_inputs));
    let msg = crate::steerer::fleet::judge::build_judge_user_message(
        &eval_inputs,
        None,
        VerifierInputBudget::V1,
    );
    assert!(
        !msg.contains("All done, finished."),
        "the capped message drops the early claim that the gate still saw"
    );
    Ok(())
}

fn gate_ctx() -> anyhow::Result<FleetCtx> {
    let llm = OpenRouterClient::new("sk-test".to_string(), "m".to_string(), None, None)?;
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    Ok(make_fleet_ctx(store, None, llm))
}

#[test]
fn arming_done_claim_reads_whole_text() {
    let text = long_text_with_early_done_claim();
    assert!(has_explicit_done_claim(&text));
    let i = EvalInputs {
        eval_text: text,
        ..Default::default()
    };
    let capped = VerifierInputBudget::V1.apply(&i);
    assert!(!has_explicit_done_claim(&capped.eval_text));
}

#[test]
fn non_verifier_steerer_gets_uncapped() {
    let settings = Arc::new(arc_swap::ArcSwap::new(overlay_with_budget(
        "verifier-input-v1",
    )));
    assert_eq!(
        VerifierInputBudget::for_steerer_kind(&settings, "refocus"),
        VerifierInputBudget::Uncapped
    );
    assert_eq!(
        VerifierInputBudget::for_steerer_kind(&settings, "verifier"),
        VerifierInputBudget::V1
    );
}
