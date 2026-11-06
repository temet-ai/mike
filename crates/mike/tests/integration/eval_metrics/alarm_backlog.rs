use mike::store::persistence::eval_metrics::{AlarmKind, AlarmThresholds, Firing};
use mike::store::persistence::turn_verdicts::VerdictMode;

use super::set_setting;
use crate::judge_quality::promotion_support::{
    bundle, closed_turn, facts, resolved_verdict as verdict,
};
use crate::turn_facts::{Fixture, fixture};

fn thresholds() -> AlarmThresholds {
    AlarmThresholds {
        mix_min_turns: 50,
        mix_min_tvd: 0.20,
        mix_min_chi2: 40.0,
        backlog_min_turns: 1,
        stalled_min_waiting: 1_000,
        stalled_hours: 6,
        refusals_min_share: 0.5,
        refusals_min_refused: 50,
        gate_grace_hours: 24,
        call_loop_min_calls: 50,
        call_loop_max_calls_per_row: 3.0,
        session_judge_max_attempts_per_day: 3,
        band_enabled: false,
        band_min: 0.32,
        band_max: 0.49,
        shadow_disagreement_enabled: true,
        shadow_disagreement_min_n: 50,
        shadow_disagreement_min_kappa: 0.80,
        metric_drop_enabled: true,
        metric_drop_min_n: 20,
    }
}

fn roles() -> Vec<String> {
    vec!["turn_judge".to_owned()]
}

async fn turn_closed_hours_ago(
    f: &Fixture,
    start: i64,
    prompt_kind: &str,
    hours: i32,
) -> anyhow::Result<i64> {
    let turn = closed_turn(f, start).await?;
    facts(f, turn, prompt_kind, "resolved").await?;
    sqlx::query(
        "UPDATE memory.operator_turns
            SET closed_at = now() - make_interval(hours => $2)
          WHERE id = $1",
    )
    .bind(turn)
    .bind(hours)
    .execute(f.handle.pool())
    .await?;
    Ok(turn)
}

fn backlog(firings: &[Firing]) -> Option<&Firing> {
    firings
        .iter()
        .find(|x| x.kind == AlarmKind::UnjudgedBacklog)
}

#[tokio::test]
async fn unjudged_backlog_counts_only_what_the_live_judge_owes() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    set_setting(&f, "turn_judge.live_bundle_id", serde_json::json!(b)).await?;
    set_setting(&f, "turn_judge.live_window_days", serde_json::json!(7)).await?;
    let owed = turn_closed_hours_ago(&f, 23_101, "genuine", 10).await?;
    turn_closed_hours_ago(&f, 23_102, "loop-wakeup", 10).await?;
    turn_closed_hours_ago(&f, 23_103, "genuine", 24 * 30).await?;
    let judged = turn_closed_hours_ago(&f, 23_104, "genuine", 10).await?;
    f.handle
        .insert_turn_verdict(verdict(
            judged,
            VerdictMode::Live,
            b,
            ("accepted", "delivered"),
            0.9,
        ))
        .await?;
    let firings = f
        .handle
        .evaluate_operational_alarms(&thresholds(), &roles())
        .await?;
    let fired =
        backlog(&firings).ok_or_else(|| anyhow::anyhow!("no backlog firing: {firings:?}"))?;
    assert_eq!(
        fired.detail["stale_turns"],
        serde_json::json!(1),
        "only turn {owed} is owed a verdict: {firings:?}"
    );
    Ok(())
}

#[tokio::test]
async fn unjudged_backlog_is_quiet_when_the_live_judge_owes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    set_setting(&f, "turn_judge.live_bundle_id", serde_json::json!(b)).await?;
    turn_closed_hours_ago(&f, 23_201, "loop-wakeup", 10).await?;
    turn_closed_hours_ago(&f, 23_202, "genuine", 24 * 30).await?;
    let firings = f
        .handle
        .evaluate_operational_alarms(&thresholds(), &roles())
        .await?;
    assert!(backlog(&firings).is_none(), "{firings:?}");
    Ok(())
}

#[tokio::test]
async fn unjudged_backlog_uses_the_older_count_without_a_live_configuration() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    turn_closed_hours_ago(&f, 23_301, "loop-wakeup", 10).await?;
    let firings = f
        .handle
        .evaluate_operational_alarms(&thresholds(), &roles())
        .await?;
    let fired =
        backlog(&firings).ok_or_else(|| anyhow::anyhow!("no backlog firing: {firings:?}"))?;
    assert_eq!(fired.detail["stale_turns"], serde_json::json!(1));
    Ok(())
}
