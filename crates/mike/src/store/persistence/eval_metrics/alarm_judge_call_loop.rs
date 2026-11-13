use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct CallLoopRow {
    pub ok_calls: i64,
    pub refused: i64,
    pub verdict_rows: i64,
    pub sessions_at_cap: i64,
    pub top_sessions: serde_json::Value,
}

pub fn loop_fires(row: &CallLoopRow, min_calls: i64, max_per_row: f64) -> bool {
    row.ok_calls >= min_calls && row.ok_calls as f64 > max_per_row * row.verdict_rows.max(1) as f64
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn eval_session_judge_call_loop(
        self,
        at: DateTime<Utc>,
        max_attempts_per_day: i64,
    ) -> Result<CallLoopRow, PersistError> {
        self.eval_session_judge_call_loops(&[at], max_attempts_per_day)
            .await?
            .into_iter()
            .next()
            .map(|(_, row)| row)
            .ok_or_else(|| {
                PersistError::DataInvariant("the call-loop query returned no row".to_owned())
            })
    }

    async fn eval_session_judge_call_loops(
        self,
        ats: &[DateTime<Utc>],
        max_attempts_per_day: i64,
    ) -> Result<Vec<(DateTime<Utc>, CallLoopRow)>, PersistError> {
        crate::metrics::time_db("eval_session_judge_call_loop", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/eval_metrics/call_loop.sql",
                ats,
                i32::try_from(max_attempts_per_day).unwrap_or(i32::MAX),
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| {
                    (
                        r.at,
                        CallLoopRow {
                            ok_calls: r.ok_calls,
                            refused: r.refused,
                            verdict_rows: r.verdict_rows,
                            sessions_at_cap: r.sessions_at_cap,
                            top_sessions: r.top_sessions,
                        },
                    )
                })
                .collect())
        })
        .await
    }
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub(super) async fn backtest_call_loop(
        self,
        steps: &[DateTime<Utc>],
        t: &super::types::AlarmThresholds,
    ) -> Result<Vec<super::types::BacktestRow>, PersistError> {
        let rows = self
            .eval_session_judge_call_loops(steps, t.session_judge_max_attempts_per_day)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for (at, row) in rows {
            out.push(super::types::BacktestRow {
                at,
                subject: "role:session_judge".to_owned(),
                fires: loop_fires(&row, t.call_loop_min_calls, t.call_loop_max_calls_per_row),
                stats: serde_json::to_value(&row)?,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{CallLoopRow, loop_fires};

    fn row(ok: i64, rows: i64) -> CallLoopRow {
        CallLoopRow {
            ok_calls: ok,
            refused: 0,
            verdict_rows: rows,
            sessions_at_cap: 0,
            top_sessions: serde_json::json!([]),
        }
    }

    #[test]
    fn the_storm_fires_and_normal_running_does_not() {
        assert!(loop_fires(&row(2_347, 1), 50, 3.0));
        assert!(
            loop_fires(&row(240, 0), 50, 3.0),
            "calls with no rows at all"
        );
        assert!(!loop_fires(&row(5, 4), 50, 3.0));
        assert!(!loop_fires(&row(125, 100), 50, 3.0));
    }
}
