use std::time::Duration;

use sqlx::postgres::types::PgInterval;

use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn upsert_project_returning_id(
        &self,
        project_path: &str,
    ) -> Result<i64, PersistError> {
        let project_path = project_path.to_owned();
        crate::metrics::time_db("upsert_project_returning_id", async move {
            let now = chrono::Utc::now();
            let project_id: i64 = sqlx::query_scalar(
                "INSERT INTO projects.projects
                     (project_path, display_name, first_seen, last_seen)
                 VALUES ($1, $1, $2, $2)
                 ON CONFLICT (project_path) DO SELECT
                 RETURNING COALESCE(merged_into_id, id)",
            )
            .bind(&project_path)
            .bind(now)
            .fetch_one(&self.pool)
            .await?;
            Ok(project_id)
        })
        .await
    }
}

pub fn pg_interval_to_humantime(interval: PgInterval) -> Option<String> {
    let total_micros = (interval.months as i128) * 30 * 24 * 3_600 * 1_000_000
        + (interval.days as i128) * 24 * 3_600 * 1_000_000
        + (interval.microseconds as i128);
    if total_micros <= 0 {
        return None;
    }
    let secs_total = (total_micros / 1_000_000) as u64;
    let subsec_nanos = ((total_micros % 1_000_000) as u32) * 1000;
    let d = Duration::new(secs_total, subsec_nanos);
    Some(humantime::format_duration(d).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_interval_zero_maps_to_none() {
        let zero = PgInterval {
            months: 0,
            days: 0,
            microseconds: 0,
        };
        assert!(pg_interval_to_humantime(zero).is_none());
    }

    #[test]
    fn pg_interval_one_hour_maps_to_humantime_string() {
        let one_hour = PgInterval {
            months: 0,
            days: 0,
            microseconds: 3600 * 1_000_000,
        };
        let got = pg_interval_to_humantime(one_hour);
        assert_eq!(got.as_deref(), Some("1h"));
    }

    #[test]
    fn pg_interval_one_day_maps_to_humantime_string() {
        let one_day = PgInterval {
            months: 0,
            days: 1,
            microseconds: 0,
        };
        let got = pg_interval_to_humantime(one_day);
        assert_eq!(got.as_deref(), Some("1day"));
    }

    #[test]
    fn pg_interval_round_trip_through_parse_duration() -> anyhow::Result<()> {
        let one_hour = PgInterval {
            months: 0,
            days: 0,
            microseconds: 3600 * 1_000_000,
        };
        let text = pg_interval_to_humantime(one_hour)
            .ok_or_else(|| anyhow::anyhow!("expected Some(\"1h\")"))?;
        let parsed = crate::store::retention::RetentionPolicy::parse_duration(&text);
        assert_eq!(parsed, Duration::from_secs(3600));
        Ok(())
    }
}
