use mike::store::persistence::eval_metrics::{AlarmKind, AlarmThresholds, Firing};

use crate::turn_facts::{Fixture, fixture};

fn thresholds(grace_hours: i64) -> AlarmThresholds {
    AlarmThresholds {
        mix_min_turns: 50,
        mix_min_tvd: 0.20,
        mix_min_chi2: 40.0,
        backlog_min_turns: 10,
        stalled_min_waiting: 5,
        stalled_hours: 6,
        refusals_min_share: 0.5,
        refusals_min_refused: 50,
        gate_grace_hours: grace_hours,
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

async fn ungated_bundle(f: &Fixture, override_reason: Option<&str>) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha, override_reason, created_at)
         VALUES ('session_judge', '', 'resolver', 'live_ungated', '{}'::jsonb, 'sha-jc08', 'build', $1, now() - interval '1 hour')
         RETURNING id",
    )
    .bind(override_reason)
    .fetch_one(f.handle.pool())
    .await?)
}

fn roles() -> Vec<String> {
    vec!["session_judge".to_owned()]
}

#[tokio::test]
async fn change_without_gate_fires_after_grace() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = ungated_bundle(&f, None).await?;
    let firings = f
        .handle
        .evaluate_operational_alarms(&thresholds(0), &roles())
        .await?;
    let subject = format!("bundle:{id}");
    assert!(
        firings
            .iter()
            .any(|x| x.kind == AlarmKind::ChangeWithoutGate && x.subject == subject),
        "{firings:?}"
    );
    let changes = f
        .handle
        .apply_alarm_pass(&AlarmKind::OPERATIONAL, &firings, 3)
        .await?;
    assert!(changes.iter().any(|c| c.subject == subject && c.opened));
    Ok(())
}

#[tokio::test]
async fn change_without_gate_quiet_with_override_reason() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    ungated_bundle(&f, Some("operator accepted the change")).await?;
    let firings = f
        .handle
        .evaluate_operational_alarms(&thresholds(0), &roles())
        .await?;
    assert!(
        firings
            .iter()
            .all(|x| x.kind != AlarmKind::ChangeWithoutGate || !x.subject.starts_with("bundle:"))
    );
    Ok(())
}

#[tokio::test]
async fn change_without_gate_quiet_when_role_not_expected() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    ungated_bundle(&f, None).await?;
    let firings = f
        .handle
        .evaluate_operational_alarms(&thresholds(0), &[])
        .await?;
    assert!(
        firings
            .iter()
            .all(|x| x.kind != AlarmKind::ChangeWithoutGate)
    );
    Ok(())
}

#[tokio::test]
async fn change_without_gate_quiet_while_another_configuration_is_in_use() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    ungated_bundle(&f, None).await?;
    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ('session_judge.live_bundle_id', '13'::jsonb)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .execute(f.handle.pool())
    .await?;
    let firings = f
        .handle
        .evaluate_operational_alarms(&thresholds(0), &roles())
        .await?;
    assert!(
        firings
            .iter()
            .all(|x| x.kind != AlarmKind::ChangeWithoutGate || !x.subject.starts_with("bundle:")),
        "{firings:?}"
    );
    Ok(())
}

fn firing(subject: &str) -> Firing {
    Firing {
        kind: AlarmKind::JudgeStalled,
        subject: subject.to_owned(),
        detail: serde_json::json!({"waiting": 7}),
    }
}

#[tokio::test]
async fn alarm_episode_clears_after_three_quiet_passes_and_refires_as_new() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let kinds = AlarmKind::OPERATIONAL;
    f.handle
        .apply_alarm_pass(&kinds, &[firing("turn_judge")], 3)
        .await?;
    f.handle
        .apply_alarm_pass(&kinds, &[firing("turn_judge")], 3)
        .await?;
    let evaluations: i32 =
        sqlx::query_scalar("SELECT evaluations FROM eval.alarms WHERE cleared_at IS NULL")
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(evaluations, 2);
    for _ in 0..3 {
        f.handle.apply_alarm_pass(&kinds, &[], 3).await?;
    }
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM eval.alarms WHERE cleared_at IS NULL")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(open, 0);
    f.handle
        .apply_alarm_pass(&kinds, &[firing("turn_judge")], 3)
        .await?;
    let episodes: i64 = sqlx::query_scalar("SELECT count(*) FROM eval.alarms")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(episodes, 2);
    Ok(())
}

#[tokio::test]
async fn daily_kind_not_counted_quiet_by_operational_pass() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let daily = Firing {
        kind: AlarmKind::LabelMixShift,
        subject: "shift:turn_judge/legacy_outcome".to_owned(),
        detail: serde_json::json!({}),
    };
    f.handle
        .apply_alarm_pass(&AlarmKind::DAILY, &[daily], 3)
        .await?;
    for _ in 0..4 {
        f.handle
            .apply_alarm_pass(&AlarmKind::OPERATIONAL, &[], 3)
            .await?;
    }
    let quiet: i32 = sqlx::query_scalar(
        "SELECT quiet_evaluations FROM eval.alarms WHERE kind = 'label_mix_shift'",
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(quiet, 0);
    Ok(())
}

#[tokio::test]
async fn budget_detail_reads_no_secret_keys() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let keys = mike::store::persistence::eval_metrics::budget_setting_keys();
    assert!(
        keys.iter()
            .all(|k| !k.to_lowercase().contains("key") && !k.to_lowercase().contains("secret"))
    );
    let rows = f
        .handle
        .eval_refusals_at(chrono::Utc::now(), 50, 0.5)
        .await?;
    assert!(rows.is_empty());
    Ok(())
}
