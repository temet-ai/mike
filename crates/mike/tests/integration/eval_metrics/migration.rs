use crate::turn_facts::fixture;

#[tokio::test]
async fn wilson_bounds_match_reference_values() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (low, high): (f64, f64) =
        sqlx::query_as("SELECT eval.wilson_low(0.56, 25), eval.wilson_high(0.56, 25)")
            .fetch_one(f.handle.pool())
            .await?;
    assert!((low - 0.3707).abs() < 1e-3, "{low}");
    assert!((high - 0.7333).abs() < 1e-3, "{high}");
    let empty: Option<f64> = sqlx::query_scalar("SELECT eval.wilson_low(0.5, 0)")
        .fetch_one(f.handle.pool())
        .await?;
    assert!(empty.is_none());
    Ok(())
}

#[tokio::test]
async fn class_map_seed_rows_present() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval.class_map WHERE production_axis IN ('legacy_outcome', 'legacy_session_outcome')",
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(n, 3);
    let two_axis: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval.class_map WHERE production_axis IN ('reaction', 'delivery')",
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(two_axis, 11);
    Ok(())
}

async fn rollup_id(f: &crate::turn_facts::Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of)
         VALUES (DATE '2026-09-01', 'test', 'manual', now()) RETURNING id",
    )
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn eval_rates_k_above_n_is_rejected() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = rollup_id(&f).await?;
    let err = sqlx::query(
        "INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_end, role, axis, class, k, n)
         VALUES ($1, 'label_mix', 'share', 'real_turns', 'global', now(), 'turn_judge', 'legacy_outcome', 'corrected', 5, 3)",
    )
    .bind(id)
    .execute(f.handle.pool())
    .await
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(err.contains("check"), "{err}");
    Ok(())
}

#[tokio::test]
async fn eval_rates_family_metric_mismatch_is_rejected() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = rollup_id(&f).await?;
    let err = sqlx::query(
        "INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_end, role, axis, class, k, n)
         VALUES ($1, 'base_rate', 'precision', 'real_turns', 'global', now(), 'turn_judge', 'reaction', 'corrected', 1, 3)",
    )
    .bind(id)
    .execute(f.handle.pool())
    .await
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(err.contains("eval_rates_family_metric"), "{err}");
    Ok(())
}

#[tokio::test]
async fn eval_alarms_second_open_episode_same_subject_conflicts() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let insert = "INSERT INTO eval.alarms (kind, subject, detail) VALUES ('judge_stalled', 'turn_judge', '{}'::jsonb)";
    sqlx::query(insert).execute(f.handle.pool()).await?;
    assert!(sqlx::query(insert).execute(f.handle.pool()).await.is_err());
    sqlx::query("UPDATE eval.alarms SET cleared_at = now()")
        .execute(f.handle.pool())
        .await?;
    sqlx::query(insert).execute(f.handle.pool()).await?;
    Ok(())
}
