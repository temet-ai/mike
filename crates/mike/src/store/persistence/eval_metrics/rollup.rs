use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgConnection;

use super::super::{PersistError, PersistHandle};
use super::{
    sql_accuracy, sql_base_rate, sql_guard_calibration, sql_label_mix, sql_steer_calibration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollupCtx {
    pub rollup_id: i64,
    pub for_day: NaiveDate,
    pub window_end: DateTime<Utc>,
    pub labels_as_of: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Accuracy,
    LabelMix,
    BaseRate,
    SteerCalibration,
    GuardCalibration,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Self::Accuracy => "accuracy",
            Self::LabelMix => "label_mix",
            Self::BaseRate => "base_rate",
            Self::SteerCalibration => "steer_calibration",
            Self::GuardCalibration => "guard_calibration",
        }
    }
}

pub const FAMILIES: &[Family] = &[
    Family::Accuracy,
    Family::LabelMix,
    Family::BaseRate,
    Family::SteerCalibration,
    Family::GuardCalibration,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollupReport {
    pub for_day: NaiveDate,
    pub rollup_id: Option<i64>,
    pub already_rolled: bool,
    pub executed: bool,
    pub per_family: Vec<(&'static str, u64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollupRequest<'a> {
    pub for_day: NaiveDate,
    pub mode: &'a str,
    pub sql_version: &'a str,
    pub execute: bool,
}

async fn run_family(
    conn: &mut PgConnection,
    family: Family,
    ctx: &RollupCtx,
) -> Result<u64, PersistError> {
    match family {
        Family::Accuracy => sql_accuracy::insert(conn, ctx).await,
        Family::LabelMix => sql_label_mix::insert(conn, ctx).await,
        Family::BaseRate => sql_base_rate::insert(conn, ctx).await,
        Family::SteerCalibration => sql_steer_calibration::insert(conn, ctx).await,
        Family::GuardCalibration => sql_guard_calibration::insert(conn, ctx).await,
    }
}

impl PersistHandle {
    pub async fn run_eval_rollup(
        &self,
        req: RollupRequest<'_>,
    ) -> Result<RollupReport, PersistError> {
        let window_end = crate::eval_metrics::day_start(req.for_day);
        let labels_as_of = Utc::now();
        crate::metrics::time_db("eval_rollup", async move {
            let mut tx = self.pool.begin().await?;
            let claimed = sqlx::query_scalar!(
                r#"INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of)
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT (for_day, sql_version) DO NOTHING
                   RETURNING id AS "id!""#,
                req.for_day,
                req.sql_version,
                req.mode,
                labels_as_of,
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(rollup_id) = claimed else {
                tx.rollback().await?;
                return Ok(RollupReport {
                    for_day: req.for_day,
                    rollup_id: None,
                    already_rolled: true,
                    executed: false,
                    per_family: Vec::new(),
                });
            };
            let ctx = RollupCtx {
                rollup_id,
                for_day: req.for_day,
                window_end,
                labels_as_of,
            };
            let mut per_family = Vec::new();
            let mut total: u64 = 0;
            for family in FAMILIES {
                let rows = run_family(&mut tx, *family, &ctx).await?;
                total += rows;
                per_family.push((family.name(), rows));
            }
            sqlx::query!(
                "UPDATE eval.rollups SET finished_at = now(), rows_written = $2 WHERE id = $1",
                rollup_id,
                i32::try_from(total).unwrap_or(i32::MAX),
            )
            .execute(&mut *tx)
            .await?;
            if req.execute {
                tx.commit().await?;
            } else {
                tx.rollback().await?;
            }
            Ok(RollupReport {
                for_day: req.for_day,
                rollup_id: req.execute.then_some(rollup_id),
                already_rolled: false,
                executed: req.execute,
                per_family,
            })
        })
        .await
    }

    pub async fn latest_rollup_day(
        &self,
        sql_version: &str,
    ) -> Result<Option<NaiveDate>, PersistError> {
        crate::metrics::time_db("eval_latest_rollup_day", async move {
            let day = sqlx::query_scalar!(
                "SELECT max(for_day) FROM eval.rollups WHERE sql_version = $1 AND finished_at IS NOT NULL",
                sql_version,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(day)
        })
        .await
    }

    pub async fn eval_setting_rows(
        &self,
    ) -> Result<Vec<(String, serde_json::Value)>, PersistError> {
        crate::metrics::time_db("eval_settings_rows", async move {
            let rows = sqlx::query!(
                "SELECT key, value FROM config.settings
                  WHERE key LIKE 'eval.rollup.%' OR key LIKE 'eval.alarm.%'",
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| (r.key, r.value)).collect())
        })
        .await
    }
}
