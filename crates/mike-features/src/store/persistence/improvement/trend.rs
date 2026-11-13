use chrono::{DateTime, Duration, NaiveTime, Utc};

use crate::store::persistence::PersistError;
use crate::store::persistence::eval_metrics::sql_label_mix::MixWindowRow;

#[derive(Debug, Clone, PartialEq)]
pub struct WeeklyShare {
    pub week_end: DateTime<Utc>,
    pub k: i64,
    pub n: i64,
    pub judges: serde_json::Value,
}

pub fn week_ends(now: DateTime<Utc>, weeks: i64) -> Vec<DateTime<Utc>> {
    let last = now.date_naive().and_time(NaiveTime::MIN).and_utc();
    (0..weeks)
        .rev()
        .map(|i| last - Duration::weeks(i))
        .collect()
}

pub fn weekly_shares(ends: &[DateTime<Utc>], rows: &[MixWindowRow]) -> Vec<WeeklyShare> {
    ends.iter()
        .map(|end| {
            let current = rows.iter().find(|r| r.d == *end && r.win == "current");
            WeeklyShare {
                week_end: *end,
                k: current.map_or(0, |r| r.class_count("corrected")),
                n: current.map_or(0, |r| r.n),
                judges: current.map_or_else(|| serde_json::json!([]), |r| r.bundles.clone()),
            }
        })
        .collect()
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn list_weekly_corrected_share(
        self,
        now: DateTime<Utc>,
        weeks: i64,
    ) -> Result<Vec<WeeklyShare>, PersistError> {
        let ends = week_ends(now, weeks);
        let rows = self.label_mix_windows(&ends).await?;
        Ok(weekly_shares(&ends, &rows))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn row(d: DateTime<Utc>, win: &str, corrected: i64, n: i64) -> MixWindowRow {
        MixWindowRow {
            d,
            win: win.to_owned(),
            n,
            classes: json!({ "corrected": corrected, "accepted": n - corrected }),
            bundles: json!([{ "label": "bundle:14", "bundle_id": 14, "count": n }]),
            replied_n: 0,
            replied_corrected: 0,
        }
    }

    #[test]
    fn week_ends_step_back_from_midnight_today() {
        let now = Utc.with_ymd_and_hms(2026, 9, 29, 14, 30, 0).unwrap();
        let ends = week_ends(now, 3);
        assert_eq!(
            ends,
            vec![
                Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 9, 22, 0, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 9, 29, 0, 0, 0).unwrap(),
            ]
        );
    }

    #[test]
    fn weekly_shares_read_the_current_window_and_keep_empty_weeks() {
        let ends = week_ends(Utc.with_ymd_and_hms(2026, 9, 29, 1, 0, 0).unwrap(), 2);
        let rows = vec![
            row(ends[1], "baseline", 20, 900),
            row(ends[1], "current", 71, 283),
        ];
        let shares = weekly_shares(&ends, &rows);
        assert_eq!(shares.len(), 2);
        assert_eq!((shares[0].k, shares[0].n), (0, 0));
        assert_eq!(shares[0].judges, json!([]));
        assert_eq!((shares[1].k, shares[1].n), (71, 283));
        assert_eq!(shares[1].judges[0]["bundle_id"], 14);
    }
}
