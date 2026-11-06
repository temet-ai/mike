use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct RejudgeCandidate {
    pub session_db_id: i64,
    pub session_uuid: String,
    pub end_turn_id: i64,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionRejudgeWindow<'a> {
    pub bundle_id: i64,
    pub since: chrono::DateTime<chrono::Utc>,
    pub until: chrono::DateTime<chrono::Utc>,
    pub judge_model: &'a str,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct V3Candidate {
    pub session_db_id: i64,
    pub session_uuid: String,
    pub end_turn_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct V3Selection<'a> {
    pub judge_model: &'a str,
    pub bundle_id: i64,
    pub lookback_hours: i32,
    pub max_attempts_per_day: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowSelection<'a> {
    pub judge_model: &'a str,
    pub bundle_id: i64,
    pub lookback_hours: i32,
    pub max_attempts_per_day: i64,
    pub limit: i64,
}

impl PersistHandle {
    pub async fn fetch_sessions_needing_v3(
        &self,
        q: &V3Selection<'_>,
    ) -> Result<Vec<V3Candidate>, PersistError> {
        crate::metrics::time_db("fetch_sessions_needing_v3", async move {
            Ok(sqlx::query_file_as!(
                V3Candidate,
                "src/store/persistence/session_end/v3_candidates.sql",
                q.judge_model,
                q.bundle_id,
                q.lookback_hours,
                q.max_attempts_per_day,
                q.limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fetch_sessions_needing_shadow(
        &self,
        q: &ShadowSelection<'_>,
    ) -> Result<Vec<V3Candidate>, PersistError> {
        crate::metrics::time_db("fetch_sessions_needing_shadow", async move {
            Ok(sqlx::query_file_as!(
                V3Candidate,
                "src/store/persistence/session_end/shadow_candidates.sql",
                q.judge_model,
                q.bundle_id,
                q.lookback_hours,
                q.max_attempts_per_day,
                q.limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn session_rejudge_candidates(
        &self,
        w: &SessionRejudgeWindow<'_>,
        limit: i64,
    ) -> Result<Vec<RejudgeCandidate>, PersistError> {
        crate::metrics::time_db("session_rejudge_candidates", async move {
            Ok(sqlx::query_file_as!(
                RejudgeCandidate,
                "src/store/persistence/session_end/rejudge_candidates.sql",
                w.bundle_id,
                w.since,
                w.until,
                limit,
                w.judge_model,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn session_v3_mean_cost(&self, bundle_id: i64) -> Result<Option<f64>, PersistError> {
        crate::metrics::time_db("session_v3_mean_cost", async move {
            Ok(sqlx::query_scalar!(
                r#"WITH recent AS (
                       SELECT cost_usd FROM session_judges.verdicts
                        WHERE bundle_id = $1 AND mode = 'live' AND cost_usd IS NOT NULL
                        ORDER BY id DESC LIMIT 200
                   )
                   SELECT avg(cost_usd)::float8 AS "mean_cost" FROM recent"#,
                bundle_id,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn session_rejudge_calls_today(
        &self,
        w: &SessionRejudgeWindow<'_>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("session_rejudge_calls_today", async move {
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/session_end/rejudge_today.sql",
                w.bundle_id,
                w.since,
                w.until,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
