use mike::store::persistence::eval_metrics::{AckOutcome, AlarmState, AlarmsFilter, RatesFilter};

use crate::turn_facts::fixture;

#[tokio::test]
async fn alarms_read_returns_detail_unmodified_and_acknowledge_round_trips() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let detail = serde_json::json!({"nested": {"text": "x".repeat(5_000)}, "n": 7});
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO eval.alarms (kind, subject, detail) VALUES ('judge_stalled', 'turn_judge', $1) RETURNING id",
    )
    .bind(&detail)
    .fetch_one(f.handle.pool())
    .await?;
    let (rows, total) = f
        .handle
        .list_eval_alarms(&AlarmsFilter {
            state: AlarmState::Open,
            limit: 10,
            ..AlarmsFilter::default()
        })
        .await?;
    assert_eq!(total, 1);
    assert_eq!(rows[0].detail, detail);
    let (outcome, row) = f
        .handle
        .acknowledge_eval_alarm(id, "seen it", false)
        .await?;
    assert_eq!(outcome, AckOutcome::Acknowledged);
    assert!(row.is_some_and(|r| r.acknowledged_at.is_some() && r.cleared_at.is_none()));
    let (again, _) = f.handle.acknowledge_eval_alarm(id, "again", false).await?;
    assert_eq!(again, AckOutcome::AlreadyAcknowledged);
    let (missing, _) = f
        .handle
        .acknowledge_eval_alarm(987_654, "none", false)
        .await?;
    assert_eq!(missing, AckOutcome::NotFound);
    Ok(())
}

#[tokio::test]
async fn rates_read_paginates_complete_rows_with_total() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let rollup: i64 = sqlx::query_scalar(
        "INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of, finished_at)
         VALUES (DATE '2026-09-20', 'test', 'manual', now(), now()) RETURNING id",
    )
    .fetch_one(f.handle.pool())
    .await?;
    for class in ["accepted", "corrected", "abandoned", "shipped", "unknown"] {
        sqlx::query(
            "INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_end, role, axis, class, k, n)
             VALUES ($1, 'label_mix', 'share', 'real_turns', 'global', now(), 'turn_judge', 'legacy_outcome', $2, 1, 4)",
        )
        .bind(rollup)
        .bind(class)
        .execute(f.handle.pool())
        .await?;
    }
    let mut seen = Vec::new();
    for offset in [0, 2, 4] {
        let (rows, total) = f
            .handle
            .list_eval_rates(&RatesFilter {
                latest: true,
                limit: 2,
                offset,
                ..RatesFilter::default()
            })
            .await?;
        assert_eq!(total, 5);
        seen.extend(rows.into_iter().map(|r| r.id));
    }
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), 5);
    Ok(())
}
