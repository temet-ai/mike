use chrono::{DateTime, Utc};
use serde_json::json;

use super::super::{PersistError, PersistHandle};
use super::band::band_stats;
use super::shift::shift_stats;
use super::types::{AlarmKind, AlarmThresholds, BacktestRow};

const MIX_CHUNK_DAYS: usize = 7;

struct JudgeHour {
    h: DateTime<Utc>,
    stale_6h: i64,
    waiting_30m: i64,
    verdicts: i64,
}

impl PersistHandle {
    async fn backtest_label_mix(
        &self,
        steps: &[DateTime<Utc>],
        t: &AlarmThresholds,
    ) -> Result<Vec<BacktestRow>, PersistError> {
        let mut out = Vec::new();
        for chunk in steps.chunks(MIX_CHUNK_DAYS) {
            let rows = self.label_mix_windows(chunk).await?;
            for at in chunk {
                let cur = rows.iter().find(|r| r.d == *at && r.win == "current");
                let base = rows.iter().find(|r| r.d == *at && r.win == "baseline");
                let (Some(cur), Some(base)) = (cur, base) else {
                    continue;
                };
                let stats = shift_stats(cur, base, t);
                out.push(BacktestRow {
                    at: *at,
                    subject: "shift:turn_judge/legacy_outcome".to_owned(),
                    fires: stats.fires(),
                    stats: serde_json::to_value(&stats)?,
                });
            }
        }
        Ok(out)
    }

    async fn backtest_label_band(
        &self,
        steps: &[DateTime<Utc>],
        t: &AlarmThresholds,
    ) -> Result<Vec<BacktestRow>, PersistError> {
        let mut out = Vec::new();
        for chunk in steps.chunks(MIX_CHUNK_DAYS) {
            let rows = self.label_mix_windows(chunk).await?;
            for at in chunk {
                let Some(cur) = rows.iter().find(|r| r.d == *at && r.win == "current") else {
                    continue;
                };
                let stats = band_stats(cur, t);
                out.push(BacktestRow {
                    at: *at,
                    subject: "band:turn_judge/corrected".to_owned(),
                    fires: stats.fires(),
                    stats: serde_json::to_value(&stats)?,
                });
            }
        }
        Ok(out)
    }

    async fn backtest_judge(
        &self,
        kind: AlarmKind,
        steps: &[DateTime<Utc>],
        t: &AlarmThresholds,
    ) -> Result<Vec<BacktestRow>, PersistError> {
        let hours = sqlx::query_file_as!(
            JudgeHour,
            "src/store/persistence/eval_metrics/judge_hours.sql",
            steps,
            i32::try_from(t.stalled_hours).unwrap_or(6),
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(hours
            .into_iter()
            .map(|h| {
                let fires = match kind {
                    AlarmKind::UnjudgedBacklog => h.stale_6h >= t.backlog_min_turns,
                    _ => h.waiting_30m >= t.stalled_min_waiting && h.verdicts == 0,
                };
                BacktestRow {
                    at: h.h,
                    subject: "turn_judge".to_owned(),
                    fires,
                    stats: json!({ "stale_6h": h.stale_6h, "waiting_30m": h.waiting_30m, "verdicts": h.verdicts }),
                }
            })
            .collect())
    }

    async fn backtest_refusals(
        &self,
        steps: &[DateTime<Utc>],
        t: &AlarmThresholds,
    ) -> Result<Vec<BacktestRow>, PersistError> {
        let rows = self
            .eval_refusals_many(steps, t.refusals_min_refused, t.refusals_min_share)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(at, row)| BacktestRow {
                at,
                subject: row.subject,
                fires: row.fires,
                stats: row.detail,
            })
            .collect())
    }

    async fn backtest_gold(
        &self,
        steps: &[DateTime<Utc>],
    ) -> Result<Vec<BacktestRow>, PersistError> {
        let rows = self.eval_gold_by_agent_many(steps).await?;
        Ok(rows
            .into_iter()
            .map(|(at, row)| BacktestRow {
                at,
                subject: row.subject,
                fires: true,
                stats: row.detail,
            })
            .collect())
    }

    pub async fn backtest_eval_alarms(
        &self,
        kind: AlarmKind,
        steps: &[DateTime<Utc>],
        t: &AlarmThresholds,
    ) -> Result<Vec<BacktestRow>, PersistError> {
        match kind {
            AlarmKind::LabelMixShift => self.backtest_label_mix(steps, t).await,
            AlarmKind::LabelBand => self.backtest_label_band(steps, t).await,
            AlarmKind::UnjudgedBacklog | AlarmKind::JudgeStalled => {
                self.backtest_judge(kind, steps, t).await
            }
            AlarmKind::BudgetRefusals => self.backtest_refusals(steps, t).await,
            AlarmKind::GoldLabelByAgent => self.backtest_gold(steps).await,
            AlarmKind::ChangeWithoutGate => Err(PersistError::DataInvariant(
                "change_without_gate has no as-of reconstruction: eval.bundles keeps no status history"
                    .to_owned(),
            )),
            AlarmKind::JudgeCallLoop => self.backtest_call_loop(steps, t).await,
            AlarmKind::EmbeddingDrift => Err(PersistError::DataInvariant(
                "embedding_drift is backtested by mike semantic backtest drift".to_owned(),
            )),
            AlarmKind::SteerQuestionEligible
            | AlarmKind::SteerQuestionDemoted
            | AlarmKind::SteerHoldbackHarm
            | AlarmKind::SteerWrongWakeCeiling => Err(PersistError::DataInvariant(
                "steer_* alarms are raised by the turn guard's demotion check; mike steer-questions effect --as-of reconstructs their evidence"
                    .to_owned(),
            )),
            AlarmKind::ShadowDisagreement | AlarmKind::MetricDrop => Err(PersistError::DataInvariant(
                "shadow_disagreement and metric_drop read only the current live and shadow bundles; \
                 they have no as-of reconstruction"
                    .to_owned(),
            )),
        }
    }
}
