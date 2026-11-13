use chrono::{Duration, Utc};
use mike::eval::rejudge::{RejudgeError, plan_rejudge};
use mike::settings::SettingsOverlay;
use mike::store::persistence::eval_rejudge::{NewRejudgeRun, RejudgeWindow};
use mike::store::persistence::eval_repo::EvalPersist as _;

use super::promotion_support::{bundle, closed_turn, facts};
use crate::turn_facts::{Fixture, fixture};

fn window(bundle_id: i64) -> RejudgeWindow {
    RejudgeWindow {
        bundle_id,
        since: Utc::now() - Duration::days(30),
        until: Utc::now() + Duration::hours(1),
    }
}

fn run(bundle_id: i64) -> NewRejudgeRun<'static> {
    NewRejudgeRun {
        window: window(bundle_id),
        daily_call_cap: 300,
        estimated_calls: 1,
        estimated_usd: None,
        created_by: "test",
        role: "turn_judge",
    }
}

async fn runs(f: &Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM eval.rejudge_runs")
        .fetch_one(f.handle.pool())
        .await?)
}

#[tokio::test]
async fn rejudge_run_rejects_second_active_run() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    f.handle.eval().create_rejudge_run(&run(b)).await?;
    assert!(f.handle.eval().create_rejudge_run(&run(b)).await.is_err());
    assert_eq!(runs(&f).await?, 1);
    Ok(())
}

#[tokio::test]
async fn rejudge_progress_counts_eligible_and_ineligible() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let real = closed_turn(&f, 23_001).await?;
    facts(&f, real, "genuine", "resolved").await?;
    let machine = closed_turn(&f, 23_002).await?;
    facts(&f, machine, "background-event", "resolved").await?;
    let progress = f.handle.eval().rejudge_progress(&window(b)).await?;
    assert_eq!(progress.eligible_total, 1);
    assert_eq!(progress.ineligible_total, 1);
    assert_eq!(progress.remaining, 1);
    assert_eq!(progress.calls_today, 0);
    let ineligible = f
        .handle
        .eval()
        .rejudge_ineligible_turn_ids(&window(b), 0, 500)
        .await?;
    assert_eq!(ineligible, vec![machine]);
    Ok(())
}

#[tokio::test]
async fn rejudge_refuses_without_live_bundle_and_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let overlay = SettingsOverlay::default();
    let result = plan_rejudge(
        &f.handle,
        &overlay,
        Utc::now() - Duration::days(30),
        Utc::now(),
        None,
    )
    .await;
    assert!(matches!(result, Err(RejudgeError::NoLiveBundle)));
    let b = bundle(&f).await?;
    let ungated = SettingsOverlay {
        turn_judge_live_bundle_id: b,
        ..SettingsOverlay::default()
    };
    let result = plan_rejudge(
        &f.handle,
        &ungated,
        Utc::now() - Duration::days(30),
        Utc::now(),
        None,
    )
    .await;
    assert!(matches!(result, Err(RejudgeError::Ungated(id)) if id == b));
    assert_eq!(runs(&f).await?, 0);
    Ok(())
}

async fn live_verdict(
    f: &Fixture,
    turn: i64,
    bundle_id: i64,
    at: chrono::DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.turn_verdicts (operator_turn_id, session_id, mode, bundle_id, rubric_version,
             input_builder_version, model, reaction, delivery, confidence, rationale, unmet_ask,
             evidence_quality, as_of, input_text, cost_usd, created_at, next_genuine_request_id)
         SELECT t.id, t.session_id, 'live', $2, 'v2', 'judge-input-v2.1', 'm', 'accepted', 'delivered',
                0.9, '', '', 'asserted', now(), 'input', 0.01, $3, 2
           FROM memory.operator_turns t WHERE t.id = $1",
    )
    .bind(turn)
    .bind(bundle_id)
    .bind(at)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn rejudge_calls_today_counts_only_calls_made_since_the_run_was_created() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let earlier = closed_turn(&f, 23_101).await?;
    facts(&f, earlier, "genuine", "resolved").await?;
    let later = closed_turn(&f, 23_102).await?;
    facts(&f, later, "genuine", "resolved").await?;
    let created = f.base + Duration::minutes(1);
    let window = RejudgeWindow {
        bundle_id: b,
        since: f.base - Duration::days(1),
        until: created,
    };
    live_verdict(&f, earlier, b, created - Duration::seconds(10)).await?;
    live_verdict(&f, later, b, Utc::now()).await?;
    let progress = f.handle.eval().rejudge_progress(&window).await?;
    assert_eq!(progress.judged, 2);
    assert_eq!(progress.calls_today, 1);
    assert!((progress.cost_usd_so_far - 0.01).abs() < 1e-9);
    Ok(())
}
