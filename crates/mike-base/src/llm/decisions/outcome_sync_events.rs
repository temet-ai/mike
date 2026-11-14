use crate::store::persistence::PersistError;

impl super::DecisionStore<'_> {
    pub(super) async fn sync_event_outcomes(&self) -> Result<u64, PersistError> {
        let fleet = self.sync_fleet_event_outcomes().await?;
        let sessions = self.sync_session_event_outcomes().await?;
        Ok(fleet + sessions)
    }

    async fn sync_fleet_event_outcomes(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("sync_fleet_event_outcomes", async move {
            let gate = sqlx::query_file!("src/llm/decisions/sql/outcomes/gate_blame_next_pass.sql")
                .execute(self.0.pool())
                .await?
                .rows_affected();
            let problems =
                sqlx::query_file!("src/llm/decisions/sql/outcomes/problem_solver_reopened.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected();
            let leaves =
                sqlx::query_file!("src/llm/decisions/sql/outcomes/leaf_judge_acceptance.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected();
            let wakes = sqlx::query_file!("src/llm/decisions/sql/outcomes/wake_acted_on.sql")
                .execute(self.0.pool())
                .await?
                .rows_affected();
            Ok(gate + problems + leaves + wakes)
        })
        .await
    }

    async fn sync_session_event_outcomes(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("sync_session_event_outcomes", async move {
            let probes = sqlx::query_file!("src/llm/decisions/sql/outcomes/probe_later_turns.sql")
                .execute(self.0.pool())
                .await?
                .rows_affected();
            let recalls =
                sqlx::query_file!("src/llm/decisions/sql/outcomes/recall_topic_verdicts.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected();
            Ok(probes + recalls)
        })
        .await
    }
}
