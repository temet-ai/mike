use super::outcome_sync::owned;
use crate::store::persistence::PersistError;

pub const PRECHECK_POINTS: [&str; 2] = ["verifier_precheck", "turn_guard_precheck"];

impl super::DecisionStore<'_> {
    pub(super) async fn sync_anchored_outcomes(&self) -> Result<u64, PersistError> {
        let prod = self.sync_prod_detect_anchor().await?;
        let prechecks = self.sync_precheck_decisions().await?;
        let checklists = self.sync_steerer_nudge_replies().await?;
        let judges = self.sync_turn_judge_gold_axes().await?;
        Ok(prod + prechecks + checklists + judges)
    }

    async fn sync_prod_detect_anchor(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("sync_prod_detect_anchor", async move {
            Ok(
                sqlx::query_file!("src/llm/decisions/sql/outcomes/prod_detect_message_anchor.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected(),
            )
        })
        .await
    }

    async fn sync_precheck_decisions(&self) -> Result<u64, PersistError> {
        let points = owned(&PRECHECK_POINTS);
        crate::metrics::time_db("sync_precheck_decisions", async move {
            Ok(sqlx::query_file!(
                "src/llm/decisions/sql/outcomes/precheck_decision.sql",
                &points
            )
            .execute(self.0.pool())
            .await?
            .rows_affected())
        })
        .await
    }

    async fn sync_steerer_nudge_replies(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("sync_steerer_nudge_replies", async move {
            Ok(sqlx::query_file!(
                "src/llm/decisions/sql/outcomes/steerer_checklist_nudge_reply.sql"
            )
            .execute(self.0.pool())
            .await?
            .rows_affected())
        })
        .await
    }

    async fn sync_turn_judge_gold_axes(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("sync_turn_judge_gold_axes", async move {
            Ok(
                sqlx::query_file!("src/llm/decisions/sql/outcomes/turn_judge_gold_axes.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected(),
            )
        })
        .await
    }
}
