use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatApplied {
    pub seat: String,
    pub applied: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SeatDecision {
    pub id: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub seat: String,
    pub point: String,
    pub applied: Option<String>,
    pub fail_open: Option<bool>,
    pub verdict: Option<serde_json::Value>,
}

impl super::DecisionStore<'_> {
    pub async fn recent_fleet_decisions(
        &self,
        fleet_run_id: i64,
        points: &[String],
        limit: i64,
    ) -> Result<Vec<SeatDecision>, PersistError> {
        crate::metrics::time_db("recent_fleet_decisions", async move {
            Ok(sqlx::query_as!(
                SeatDecision,
                r#"SELECT id AS "id!", created_at AS "created_at!",
                          COALESCE(seat, '') AS "seat!", point AS "point!",
                          applied, fail_open, composed -> 'verdict' AS verdict
                     FROM decisions.records
                    WHERE fleet_run_id = $1 AND point = ANY($2) AND mode IN ('on', 'shadow')
                    ORDER BY created_at DESC, id DESC
                    LIMIT $3"#,
                fleet_run_id,
                points,
                limit,
            )
            .fetch_all(self.0.pool())
            .await?)
        })
        .await
    }

    pub async fn applied_counts_by_seat(
        &self,
        fleet_run_id: i64,
        point: &str,
    ) -> Result<Vec<SeatApplied>, PersistError> {
        crate::metrics::time_db("decision_applied_counts_by_seat", async move {
            let rows = sqlx::query!(
                r#"SELECT COALESCE(seat, '') AS "seat!", COALESCE(applied, 'none') AS "applied!",
                          count(*) AS "count!"
                     FROM decisions.records
                    WHERE fleet_run_id = $1 AND point = $2 AND mode = 'on'
                    GROUP BY 1, 2
                    ORDER BY 1, 2"#,
                fleet_run_id,
                point,
            )
            .fetch_all(self.0.pool())
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| SeatApplied {
                    seat: r.seat,
                    applied: r.applied,
                    count: r.count,
                })
                .collect())
        })
        .await
    }
}
