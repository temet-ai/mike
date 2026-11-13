use chrono::{Duration, Utc};
use mike::store::persistence::cross_session::ThresholdWrite;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use serde_json::json;

use crate::turn_facts::{Fixture, fixture};

async fn rollup(f: &Fixture, day_offset: i64, finished_hours_ago: i64) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of, finished_at)
         VALUES (current_date - $1::int, 'jc18-v1', 'manual', now(), now() - make_interval(hours => $2::int))
         RETURNING id",
    )
    .bind(i32::try_from(day_offset)?)
    .bind(i32::try_from(finished_hours_ago)?)
    .fetch_one(f.handle.pool())
    .await?)
}

fn write(steerer: i64, rollup_id: i64, decision: &str, applied: f32) -> ThresholdWrite {
    ThresholdWrite {
        steerer_id: steerer,
        question: "unbacked_wait".to_owned(),
        scope: "global".to_owned(),
        project_id: None,
        rollup_id,
        rollup_finished_at: Utc::now(),
        window_start: Utc::now() - Duration::days(30),
        window_end: Utc::now(),
        label_basis: "auto_shadow_gold_relayed".to_owned(),
        question_mode: "shadow".to_owned(),
        threshold_mode: "apply".to_owned(),
        n_labelled: 40,
        precision_bar: 0.9,
        threshold_floor: 0.6,
        threshold_ceiling: 0.95,
        static_threshold: 0.75,
        candidate_threshold: Some(applied),
        applied_threshold: applied,
        decision: decision.to_owned(),
        detail: json!({"previous_threshold": 0.75, "bins": []}),
    }
}

#[tokio::test]
async fn snapshot_as_of_t_ignores_rollups_after_t() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let older = rollup(&f, 2, 48).await?;
    let newer = rollup(&f, 1, 1).await?;
    sqlx::query(
        "INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_end, role,
                                 axis, class, k, n)
         SELECT r, 'steer_calibration', 'fire_rate', 'steerer_evaluations', 'global', now(),
                'steerer', 'steer_valid', 'refocus', 0, 0
           FROM unnest($1::bigint[]) AS r",
    )
    .bind(vec![older, newer])
    .execute(f.handle.pool())
    .await?;
    let at = Utc::now() - Duration::hours(24);
    let seen = f.handle.steerer().rollup_as_of(at).await?;
    assert_eq!(seen.map(|r| r.id), Some(older));
    assert_eq!(
        f.handle
            .steerer()
            .rollup_as_of(Utc::now())
            .await?
            .map(|r| r.id),
        Some(newer)
    );
    Ok(())
}

#[tokio::test]
async fn threshold_pass_in_apply_mode_writes_one_row_and_one_calibration_change()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let steerer = f
        .handle
        .steerer()
        .guard_steerer()
        .await?
        .map(|g| g.id)
        .unwrap_or_default();
    let r = rollup(&f, 1, 1).await?;
    let changes = f
        .handle
        .steerer()
        .insert_threshold_rows(&[
            write(steerer, r, "applied", 0.85),
            write(steerer, r, "held_hysteresis", 0.75),
        ])
        .await?;
    assert_eq!(changes.len(), 1, "only the applied row writes a change");
    let (via, kind, detail): (String, String, serde_json::Value) =
        sqlx::query_as("SELECT via, subject_kind, detail FROM config.change_log WHERE id = $1")
            .bind(changes[0])
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(
        (via.as_str(), kind.as_str()),
        ("calibration", "guard_threshold")
    );
    assert!(detail["threshold_row_id"].as_i64().is_some());
    let in_force = f
        .handle
        .steerer()
        .thresholds_in_force(steerer, Utc::now() + Duration::minutes(1))
        .await?;
    assert_eq!(in_force.len(), 1);
    assert!((in_force[0].applied_threshold - 0.85).abs() < 1e-6);
    Ok(())
}
