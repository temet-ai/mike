use crate::store::persistence::state_repo::StatePersist as _;
use chrono::{Duration, TimeZone};

use super::*;
use crate::config::db_cost::DbCostConfig;
use crate::db_cost::report::ReportStatus;
use crate::db_cost::worker::{Monitor, tick};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::TestDb;

async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire().await?;
    Ok(Some(PersistHandle::connect(&db.database_url).await?))
}

#[tokio::test]
async fn count_requests_window_with_no_traffic_is_zero() -> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    let from = Utc
        .with_ymd_and_hms(2999, 1, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("timestamp out of range"))?;
    let count = count_requests(&persist, from, from + Duration::hours(1)).await?;
    assert_eq!(count, 0);
    Ok(())
}

#[tokio::test]
async fn read_texts_with_no_ids_skips_the_query() -> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    assert!(read_texts(&persist, &[]).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn tick_first_reading_never_degrades() -> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    let mut monitor = Monitor::new(DbCostConfig::default());
    let report = tick(&persist, &mut monitor).await;
    assert!(
        matches!(
            report.status,
            ReportStatus::WarmingUp | ReportStatus::Unavailable
        ),
        "{report:?}"
    );
    if report.status == ReportStatus::Unavailable {
        assert!(!report.fix.is_empty(), "{report:?}");
    }
    Ok(())
}

#[tokio::test]
async fn read_snapshot_on_a_closed_pool_is_an_error() -> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    persist.pool().close().await;
    assert!(read_snapshot(&persist).await.is_err());
    Ok(())
}

#[tokio::test]
async fn maintenance_reports_cumulative_counters_and_reset_boundaries() -> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    let reading = persist.state().read_database_maintenance().await?;
    assert!(!reading.tables.is_empty());
    assert!(!reading.largest_indexes.is_empty());
    assert!(!reading.cluster_io.is_empty());
    assert!(
        reading
            .tables
            .iter()
            .all(|table| table.total_autovacuum_ms >= 0.0)
    );
    assert!(
        reading
            .largest_indexes
            .iter()
            .all(|index| index.bytes >= 0 && index.scans >= 0)
    );
    let json = serde_json::to_value(&reading)?;
    assert!(json["largest_indexes"][0].get("stats_reset").is_some());
    assert!(json["cluster_io"][0].get("read_bytes").is_some());
    Ok(())
}

#[tokio::test]
async fn statement_reading_tracks_generic_and_custom_prepared_plans() -> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    let preload: String = sqlx::query_scalar("SHOW shared_preload_libraries")
        .fetch_one(persist.pool())
        .await?;
    if !preload.contains("pg_stat_statements") {
        anyhow::bail!(
            "plan-counter test requires pg_stat_statements in the disposable server's shared_preload_libraries"
        );
    }
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(persist.pool())
        .await?;
    let mut connection = persist.pool().acquire().await?;
    for mode in ["force_generic_plan", "force_custom_plan"] {
        sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
            .bind(mode)
            .execute(&mut *connection)
            .await?;
        let value: i64 = sqlx::query_scalar("SELECT $1::bigint + 41 /* mike_pg19_plan_usage */")
            .bind(1_i64)
            .fetch_one(&mut *connection)
            .await?;
        assert_eq!(value, 42);
    }
    let Reading::Taken(snapshot) = read_snapshot(&persist).await? else {
        anyhow::bail!("pg_stat_statements is enabled");
    };
    let ids: Vec<i64> = snapshot.statements.keys().copied().collect();
    let texts = read_texts(&persist, &ids).await?;
    let id = texts
        .iter()
        .find(|(_, sql)| sql.contains("mike_pg19_plan_usage"))
        .map(|(id, _)| id)
        .ok_or_else(|| anyhow::anyhow!("prepared statement was not recorded"))?;
    let counters = snapshot
        .statements
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("missing counters"))?;
    assert_eq!(counters.calls, 2);
    assert_eq!(counters.generic_plan_calls, 1);
    assert_eq!(counters.custom_plan_calls, 1);
    let cost =
        crate::db_cost::budget::statement_cost(*id, *counters, 60.0, &DbCostConfig::default());
    let json = serde_json::to_value(cost)?;
    assert_eq!(json["generic_plan_calls"], 1);
    assert_eq!(json["custom_plan_calls"], 1);
    Ok(())
}
