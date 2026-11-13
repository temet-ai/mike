use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::PgConnection;

use super::super::PersistError;
use super::new_rates::{NewRate, insert_rates};
use super::rollup::RollupCtx;

pub const MIX_CLASSES: [&str; 4] = ["accepted", "corrected", "abandoned", "shipped"];

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct MixWindowRow {
    pub d: DateTime<Utc>,
    pub win: String,
    pub n: i64,
    pub classes: serde_json::Value,
    pub bundles: serde_json::Value,
    pub replied_n: i64,
    pub replied_corrected: i64,
}

impl MixWindowRow {
    pub fn class_count(&self, class: &str) -> i64 {
        self.classes
            .get(class)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    }
}

pub async fn read_windows(
    conn: &mut PgConnection,
    days: &[DateTime<Utc>],
) -> Result<Vec<MixWindowRow>, PersistError> {
    if days.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_file_as!(
        MixWindowRow,
        "src/store/persistence/eval_metrics/label_mix.sql",
        days,
    )
    .fetch_all(&mut *conn)
    .await?)
}

fn window_rates(row: &MixWindowRow, ctx: &RollupCtx) -> Vec<NewRate> {
    let (days, start) = if row.win == "current" {
        (7, ctx.window_end - Duration::days(7))
    } else {
        (28, ctx.window_end - Duration::days(35))
    };
    MIX_CLASSES
        .iter()
        .map(|class| NewRate {
            family: "label_mix",
            metric: "share",
            population: "real_turns",
            scope_kind: "global",
            window_start: Some(start),
            window_end: if row.win == "current" {
                ctx.window_end
            } else {
                ctx.window_end - Duration::days(7)
            },
            window_days: Some(days),
            role: "turn_judge",
            axis: "legacy_outcome".to_owned(),
            class: (*class).to_owned(),
            k: i32::try_from(row.class_count(class)).unwrap_or(i32::MAX),
            n: i32::try_from(row.n).unwrap_or(i32::MAX),
            detail: json!({ "window": row.win, "bundles": row.bundles }),
            ..NewRate::default()
        })
        .collect()
}

fn band_rate(row: &MixWindowRow, ctx: &RollupCtx) -> NewRate {
    NewRate {
        family: "label_mix",
        metric: "share",
        population: "replied_real_turns",
        scope_kind: "global",
        window_start: Some(ctx.window_end - Duration::days(7)),
        window_end: ctx.window_end,
        window_days: Some(7),
        role: "turn_judge",
        axis: "legacy_outcome".to_owned(),
        class: "corrected".to_owned(),
        k: i32::try_from(row.replied_corrected).unwrap_or(i32::MAX),
        n: i32::try_from(row.replied_n).unwrap_or(i32::MAX),
        detail: json!({ "window": "current", "band": true }),
        ..NewRate::default()
    }
}

pub async fn insert(conn: &mut PgConnection, ctx: &RollupCtx) -> Result<u64, PersistError> {
    let rows = read_windows(conn, &[ctx.window_end]).await?;
    let mut rates: Vec<NewRate> = rows.iter().flat_map(|r| window_rates(r, ctx)).collect();
    rates.extend(
        rows.iter()
            .filter(|r| r.win == "current")
            .map(|r| band_rate(r, ctx)),
    );
    insert_rates(conn, ctx.rollup_id, &rates).await
}
