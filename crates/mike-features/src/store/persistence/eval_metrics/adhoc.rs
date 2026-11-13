use chrono::{DateTime, Utc};

use super::super::PersistError;
use super::rollup::{Family, RollupCtx};
use super::types::EvalRateRow;
use super::{
    sql_accuracy, sql_base_rate, sql_guard_calibration, sql_label_mix, sql_steer_calibration,
};

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn eval_rates_adhoc(
        self,
        family: Family,
        window_start: Option<DateTime<Utc>>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<EvalRateRow>, PersistError> {
        let labels_as_of = Utc::now();
        crate::metrics::time_db("eval_rates_adhoc", async move {
            let mut tx = self.pool.begin().await?;
            let rollup_id = sqlx::query_scalar!(
                r#"INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of, finished_at)
                   VALUES ($1::timestamptz::date, 'adhoc:' || gen_random_uuid()::text, 'manual', $2, now())
                   RETURNING id AS "id!""#,
                window_end,
                labels_as_of,
            )
            .fetch_one(&mut *tx)
            .await?;
            let ctx = RollupCtx {
                rollup_id,
                for_day: window_end.date_naive(),
                window_end,
                labels_as_of,
            };
            match family {
                Family::Accuracy => {
                    sql_accuracy::insert_window(&mut tx, &ctx, window_start).await?;
                }
                Family::LabelMix => {
                    sql_label_mix::insert(&mut tx, &ctx).await?;
                }
                Family::BaseRate => {
                    sql_base_rate::insert(&mut tx, &ctx).await?;
                }
                Family::SteerCalibration => {
                    sql_steer_calibration::insert(&mut tx, &ctx).await?;
                }
                Family::GuardCalibration => {
                    sql_guard_calibration::insert(&mut tx, &ctx).await?;
                }
            }
            let rows = sqlx::query_file_as!(
                EvalRateRow,
                "src/store/persistence/eval_metrics/adhoc_rates.sql",
                rollup_id,
            )
            .fetch_all(&mut *tx)
            .await?;
            tx.rollback().await?;
            Ok(rows)
        })
        .await
    }
}
