use chrono::Utc;
use mike::store::persistence::eval_metrics::{AlarmKind, AlarmThresholds};
use mike::store::persistence::turn_verdicts::{
    TurnVerdictInsert, VerdictCallUsage, VerdictJudgement, VerdictMode, VerdictShape,
};
use serde_json::json;

use super::set_setting;
use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

fn thresholds(shadow_min_n: i64, shadow_min_kappa: f64, metric_min_n: i64) -> AlarmThresholds {
    AlarmThresholds {
        mix_min_turns: 50,
        mix_min_tvd: 0.20,
        mix_min_chi2: 40.0,
        backlog_min_turns: 10,
        stalled_min_waiting: 5,
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
        shadow_disagreement_min_n: shadow_min_n,
        shadow_disagreement_min_kappa: shadow_min_kappa,
        metric_drop_enabled: true,
        metric_drop_min_n: metric_min_n,
    }
}

async fn bundle(
    f: &Fixture,
    role: &str,
    origin: &str,
    components: serde_json::Value,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha)
         VALUES ($1, '', $2, 'live', $3, 'sha-' || gen_random_uuid(), 'build')
         RETURNING id",
    )
    .bind(role)
    .bind(origin)
    .bind(components)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn closed_turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, start, "add the retry and test it")).await?;
    insert_turn(f, &NewTurn::closed(start, start, start, start + 5)).await
}

fn verdict(
    turn_id: i64,
    mode: VerdictMode,
    bundle_id: i64,
    reaction: &'static str,
) -> TurnVerdictInsert<'static> {
    TurnVerdictInsert {
        turn_id,
        mode,
        bundle_id: Some(bundle_id),
        rubric_version: "v2-2026-09-23",
        input_builder_version: "judge-input-v2.1",
        slices: Vec::new(),
        model: "google/gemini-3.5-flash",
        judgement: VerdictJudgement {
            shape: VerdictShape::TwoAxis {
                reaction,
                delivery: "not_applicable",
                failure_tags: None,
                severity: None,
            },
            confidence: 0.9,
            rationale: "test fixture",
            unmet_ask: "none",
            evidence_quality: "asserted",
        },
        next_genuine_request_id: None,
        as_of: Utc::now(),
        input_text: "OPERATOR REQUEST",
        usage: VerdictCallUsage::default(),
    }
}

async fn seed_pairs(
    f: &Fixture,
    live_bundle_id: i64,
    shadow_bundle_id: i64,
    agree: i64,
    disagree: i64,
) -> anyhow::Result<()> {
    let mut start = 1_000_000;
    for _ in 0..agree {
        let turn_id = closed_turn(f, start).await?;
        f.handle
            .insert_turn_verdict(verdict(
                turn_id,
                VerdictMode::Live,
                live_bundle_id,
                "corrected",
            ))
            .await?;
        f.handle
            .insert_turn_verdict(verdict(
                turn_id,
                VerdictMode::Shadow,
                shadow_bundle_id,
                "corrected",
            ))
            .await?;
        start += 10;
    }
    for _ in 0..disagree {
        let turn_id = closed_turn(f, start).await?;
        f.handle
            .insert_turn_verdict(verdict(
                turn_id,
                VerdictMode::Live,
                live_bundle_id,
                "corrected",
            ))
            .await?;
        f.handle
            .insert_turn_verdict(verdict(
                turn_id,
                VerdictMode::Shadow,
                shadow_bundle_id,
                "accepted",
            ))
            .await?;
        start += 10;
    }
    Ok(())
}

async fn seed_accuracy_rollup(
    f: &Fixture,
    live_bundle_id: i64,
    tier: &str,
    k: i64,
    n: i64,
) -> anyhow::Result<()> {
    let rollup_id: i64 = sqlx::query_scalar(
        "INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of, started_at, finished_at, rows_written)
         VALUES (current_date, $1, 'manual', now(), now(), now(), 1)
         ON CONFLICT (for_day, sql_version) DO UPDATE SET finished_at = now()
         RETURNING id",
    )
    .bind(format!("test-{}", uuid_ish()))
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_start,
                                 window_end, window_days, role, bundle_label, axis, class,
                                 label_tier, k, n, detail)
         VALUES ($1, 'accuracy', 'precision', 'real_turns', 'global', now() - interval '30 days',
                 now(), 30, 'turn_judge', $2, 'legacy_outcome', 'corrected', $3, $4, $5, '{}'::jsonb)",
    )
    .bind(rollup_id)
    .bind(format!("bundle:{live_bundle_id}"))
    .bind(tier)
    .bind(k)
    .bind(n)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

fn uuid_ish() -> String {
    format!("{}", Utc::now().timestamp_nanos_opt().unwrap_or_default())
}

#[tokio::test]
async fn shadow_disagreement_fires_below_min_kappa_with_enough_pairs() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let components = json!({"model": "gemini-3-flash", "prompt_sha": "abc"});
    let live_id = bundle(&f, "turn_judge", "resolver", components.clone()).await?;
    set_setting(&f, "turn_judge.live_bundle_id", json!(live_id)).await?;
    let shadow_id = bundle(&f, "turn_judge_shadow", "resolver", components).await?;
    seed_pairs(&f, live_id, shadow_id, 40, 20).await?;

    let now = crate::seed::database_now(f.handle.pool()).await?;
    let firings = f
        .handle
        .evaluate_promotion_alarms(&thresholds(50, 0.80, 20), now)
        .await?;
    let subject = format!("turn_judge:{live_id}");
    let hit = firings
        .iter()
        .find(|x| x.kind == AlarmKind::ShadowDisagreement && x.subject == subject)
        .unwrap_or_else(|| panic!("expected shadow_disagreement firing, got {firings:?}"));
    assert_eq!(hit.detail["n"], json!(60));
    Ok(())
}

#[tokio::test]
async fn shadow_disagreement_silent_when_shadow_components_differ() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let live_id = bundle(
        &f,
        "turn_judge",
        "resolver",
        json!({"model": "gemini-3-flash"}),
    )
    .await?;
    set_setting(&f, "turn_judge.live_bundle_id", json!(live_id)).await?;
    let shadow_id = bundle(
        &f,
        "turn_judge_shadow",
        "resolver",
        json!({"model": "kimi-k3-candidate"}),
    )
    .await?;
    seed_pairs(&f, live_id, shadow_id, 10, 40).await?;

    let firings = f
        .handle
        .evaluate_promotion_alarms(&thresholds(50, 0.80, 20), Utc::now())
        .await?;
    assert!(
        firings
            .iter()
            .all(|x| x.kind != AlarmKind::ShadowDisagreement),
        "{firings:?}"
    );
    Ok(())
}

#[tokio::test]
async fn shadow_disagreement_silent_below_min_n() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let components = json!({"model": "gemini-3-flash"});
    let live_id = bundle(&f, "turn_judge", "resolver", components.clone()).await?;
    set_setting(&f, "turn_judge.live_bundle_id", json!(live_id)).await?;
    let shadow_id = bundle(&f, "turn_judge_shadow", "resolver", components).await?;
    seed_pairs(&f, live_id, shadow_id, 2, 8).await?;

    let firings = f
        .handle
        .evaluate_promotion_alarms(&thresholds(50, 0.80, 20), Utc::now())
        .await?;
    assert!(
        firings
            .iter()
            .all(|x| x.kind != AlarmKind::ShadowDisagreement),
        "{firings:?}"
    );
    Ok(())
}

#[tokio::test]
async fn shadow_disagreement_silent_when_disabled() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let components = json!({"model": "gemini-3-flash"});
    let live_id = bundle(&f, "turn_judge", "resolver", components.clone()).await?;
    set_setting(&f, "turn_judge.live_bundle_id", json!(live_id)).await?;
    let shadow_id = bundle(&f, "turn_judge_shadow", "resolver", components).await?;
    seed_pairs(&f, live_id, shadow_id, 40, 20).await?;

    let mut t = thresholds(50, 0.80, 20);
    t.shadow_disagreement_enabled = false;
    let firings = f.handle.evaluate_promotion_alarms(&t, Utc::now()).await?;
    assert!(
        firings
            .iter()
            .all(|x| x.kind != AlarmKind::ShadowDisagreement),
        "{firings:?}"
    );
    Ok(())
}

#[tokio::test]
async fn metric_drop_fires_when_precision_wilson_high_below_floor() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let live_id = bundle(&f, "turn_judge", "resolver", json!({})).await?;
    set_setting(&f, "turn_judge.live_bundle_id", json!(live_id)).await?;
    seed_accuracy_rollup(&f, live_id, "gold", 5, 25).await?;

    let now = crate::seed::database_now(f.handle.pool()).await?;
    let firings = f
        .handle
        .evaluate_promotion_alarms(&thresholds(50, 0.80, 20), now)
        .await?;
    let subject = format!("turn_judge:{live_id}:precision");
    assert!(
        firings
            .iter()
            .any(|x| x.kind == AlarmKind::MetricDrop && x.subject == subject),
        "{firings:?}"
    );
    Ok(())
}

#[tokio::test]
async fn metric_drop_silent_below_min_n() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let live_id = bundle(&f, "turn_judge", "resolver", json!({})).await?;
    set_setting(&f, "turn_judge.live_bundle_id", json!(live_id)).await?;
    seed_accuracy_rollup(&f, live_id, "gold", 3, 15).await?;

    let firings = f
        .handle
        .evaluate_promotion_alarms(&thresholds(50, 0.80, 20), Utc::now())
        .await?;
    assert!(
        firings.iter().all(|x| x.kind != AlarmKind::MetricDrop),
        "{firings:?}"
    );
    Ok(())
}

#[tokio::test]
async fn metric_drop_silent_when_disabled() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let live_id = bundle(&f, "turn_judge", "resolver", json!({})).await?;
    set_setting(&f, "turn_judge.live_bundle_id", json!(live_id)).await?;
    seed_accuracy_rollup(&f, live_id, "gold", 5, 25).await?;

    let mut t = thresholds(50, 0.80, 20);
    t.metric_drop_enabled = false;
    let firings = f.handle.evaluate_promotion_alarms(&t, Utc::now()).await?;
    assert!(
        firings.iter().all(|x| x.kind != AlarmKind::MetricDrop),
        "{firings:?}"
    );
    Ok(())
}
