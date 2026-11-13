use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use super::super::PersistError;
use super::band::{band_detail, band_stats};
use super::shift::{shift_detail, shift_stats};
use super::sql_label_mix::{MixWindowRow, read_windows};
use super::types::{AlarmKind, AlarmThresholds, EpisodeChange, Firing};

const STALL_WAIT_MINUTES: i64 = 30;
const BACKLOG_HOURS: i64 = 6;

fn firing(kind: AlarmKind, subject: String, detail: serde_json::Value) -> Firing {
    Firing {
        kind,
        subject,
        detail,
    }
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn apply_alarm_pass(
        self,
        kinds: &[AlarmKind],
        firings: &[Firing],
        clear_after: i32,
    ) -> Result<Vec<EpisodeChange>, PersistError> {
        let kind_codes: Vec<String> = kinds.iter().map(|k| k.code().to_owned()).collect();
        let fire_kinds: Vec<String> = firings.iter().map(|f| f.kind.code().to_owned()).collect();
        let subjects: Vec<String> = firings.iter().map(|f| f.subject.clone()).collect();
        let details: Vec<serde_json::Value> = firings.iter().map(|f| f.detail.clone()).collect();
        crate::metrics::time_db("eval_alarm_upsert", async move {
            Ok(sqlx::query_file_as!(
                EpisodeChange,
                "src/store/persistence/eval_metrics/upsert_alarms.sql",
                &kind_codes,
                clear_after,
                &fire_kinds,
                &subjects,
                &details,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    async fn turns_owed_a_verdict(
        self,
        now: DateTime<Utc>,
        closed_before: DateTime<Utc>,
    ) -> Result<(i64, Option<DateTime<Utc>>), PersistError> {
        if let Some(live) = self.live_window_backlog(now, closed_before).await? {
            return Ok((live.eligible_without_verdict, live.oldest_closed_at));
        }
        let legacy = self.unjudged_closed_turn_backlog(closed_before).await?;
        Ok((legacy.stale_turns, legacy.oldest_closed_at))
    }

    async fn judge_firings(
        self,
        t: &AlarmThresholds,
        now: DateTime<Utc>,
    ) -> Result<Vec<Firing>, PersistError> {
        let mut out = Vec::new();
        let (stale_turns, oldest_closed_at) = self
            .turns_owed_a_verdict(now, now - Duration::hours(BACKLOG_HOURS))
            .await?;
        let calls = self.eval_judge_call_outcomes(BACKLOG_HOURS).await?;
        if stale_turns >= t.backlog_min_turns {
            out.push(firing(
                AlarmKind::UnjudgedBacklog,
                "turn_judge".to_owned(),
                json!({ "stale_turns": stale_turns, "oldest_closed_at": oldest_closed_at, "calls": calls }),
            ));
        }
        let (waiting, _) = self
            .turns_owed_a_verdict(now, now - Duration::minutes(STALL_WAIT_MINUTES))
            .await?;
        let last = self.eval_last_judge_verdict_at().await?;
        let silent = last.is_none_or(|at| at < now - Duration::hours(t.stalled_hours));
        if waiting >= t.stalled_min_waiting && silent {
            out.push(firing(
                AlarmKind::JudgeStalled,
                "turn_judge".to_owned(),
                json!({ "waiting": waiting, "last_verdict_at": last, "calls": calls }),
            ));
        }
        Ok(out)
    }

    pub async fn evaluate_operational_alarms(
        self,
        t: &AlarmThresholds,
        roles: &[String],
    ) -> Result<Vec<Firing>, PersistError> {
        let now = Utc::now();
        let mut out = if roles.iter().any(|r| r == "turn_judge") {
            self.judge_firings(t, now).await?
        } else {
            Vec::new()
        };
        for row in self
            .eval_refusals_at(now, t.refusals_min_refused, t.refusals_min_share)
            .await?
            .into_iter()
            .filter(|r| r.fires)
        {
            out.push(firing(AlarmKind::BudgetRefusals, row.subject, row.detail));
        }
        if roles.iter().any(|r| r == "session_judge") {
            let row = self
                .eval_session_judge_call_loop(now, t.session_judge_max_attempts_per_day)
                .await?;
            if super::alarm_judge_call_loop::loop_fires(
                &row,
                t.call_loop_min_calls,
                t.call_loop_max_calls_per_row,
            ) {
                out.push(firing(
                    AlarmKind::JudgeCallLoop,
                    "role:session_judge".to_owned(),
                    serde_json::to_value(&row)?,
                ));
            }
        }
        for row in self.eval_ungated_changes(roles, t.gate_grace_hours).await? {
            out.push(firing(
                AlarmKind::ChangeWithoutGate,
                row.subject,
                row.detail,
            ));
        }
        Ok(out)
    }

    pub async fn label_mix_windows(
        self,
        days: &[DateTime<Utc>],
    ) -> Result<Vec<MixWindowRow>, PersistError> {
        let mut conn = self.pool.acquire().await?;
        read_windows(&mut conn, days).await
    }

    pub async fn evaluate_daily_alarms(
        self,
        t: &AlarmThresholds,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<Firing>, PersistError> {
        let rows = self.label_mix_windows(&[window_end]).await?;
        let mut out = Vec::new();
        let current = rows.iter().find(|r| r.win == "current");
        let baseline = rows.iter().find(|r| r.win == "baseline");
        if let (Some(cur), Some(base)) = (current, baseline) {
            let stats = shift_stats(cur, base, t);
            if stats.fires() {
                out.push(firing(
                    AlarmKind::LabelMixShift,
                    "shift:turn_judge/legacy_outcome".to_owned(),
                    shift_detail(cur, base, &stats),
                ));
            }
        }
        if let Some(cur) = current {
            let stats = band_stats(cur, t);
            if stats.fires() {
                out.push(firing(
                    AlarmKind::LabelBand,
                    "band:turn_judge/corrected".to_owned(),
                    band_detail(&stats),
                ));
            }
        }
        for row in self.eval_gold_by_agent_at(window_end).await? {
            out.push(firing(AlarmKind::GoldLabelByAgent, row.subject, row.detail));
        }
        out.extend(self.evaluate_promotion_alarms(t, window_end).await?);
        Ok(out)
    }
}
