use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatApplied {
    pub seat: String,
    pub applied: String,
    pub count: i64,
}

impl super::DecisionStore<'_> {
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
