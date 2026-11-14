use crate::store::persistence::PersistError;

pub const TURN_REACTION_POINTS: [&str; 7] = [
    "diagnostic_phrase",
    "text_origin",
    "directive",
    "step_class",
    "ended_on",
    "steerer_checklist",
    "prod_detect",
];

pub const NOTE_POINTS: [&str; 2] = ["commit_message", "rule_check"];

pub const LESSON_POINTS: [&str; 2] = ["lesson_shape", "topic_assign"];

fn owned(points: &[&str]) -> Vec<String> {
    points.iter().map(|p| (*p).to_owned()).collect()
}

impl super::DecisionStore<'_> {
    pub async fn sync_downstream_outcomes(&self) -> Result<u64, PersistError> {
        let mut synced = self.sync_turn_reactions().await?;
        synced += self.sync_session_and_supply_outcomes().await?;
        synced += self.sync_lesson_outcomes().await?;
        synced += self.sync_fleet_and_project_outcomes().await?;
        synced += self.sync_event_outcomes().await?;
        Ok(synced)
    }

    async fn sync_turn_reactions(&self) -> Result<u64, PersistError> {
        let points = owned(&TURN_REACTION_POINTS);
        let notes = owned(&NOTE_POINTS);
        crate::metrics::time_db("sync_turn_reaction_outcomes", async move {
            let direct = sqlx::query_file!(
                "src/llm/decisions/sql/outcomes/turn_reaction.sql",
                &points,
                false,
                "next_turn_reaction"
            )
            .execute(self.0.pool())
            .await?
            .rows_affected();
            let after_note = sqlx::query_file!(
                "src/llm/decisions/sql/outcomes/turn_reaction.sql",
                &notes,
                true,
                "next_turn_reaction_after_note"
            )
            .execute(self.0.pool())
            .await?
            .rows_affected();
            Ok(direct + after_note)
        })
        .await
    }

    async fn sync_session_and_supply_outcomes(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("sync_supply_outcomes", async move {
            let session = sqlx::query_file!("src/llm/decisions/sql/outcomes/session_turn_mix.sql")
                .execute(self.0.pool())
                .await?
                .rows_affected();
            let lessons =
                sqlx::query_file!("src/llm/decisions/sql/outcomes/lesson_supply_verdicts.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected();
            let cards =
                sqlx::query_file!("src/llm/decisions/sql/outcomes/thread_pick_card_verdicts.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected();
            let hand = sqlx::query_file!("src/llm/decisions/sql/outcomes/supply_verdict_hand.sql")
                .execute(self.0.pool())
                .await?
                .rows_affected();
            Ok(session + lessons + cards + hand)
        })
        .await
    }

    async fn sync_lesson_outcomes(&self) -> Result<u64, PersistError> {
        let points = owned(&LESSON_POINTS);
        crate::metrics::time_db("sync_lesson_outcomes", async move {
            let verdicts = sqlx::query_file!(
                "src/llm/decisions/sql/outcomes/lesson_operator_verdict.sql",
                &points
            )
            .execute(self.0.pool())
            .await?
            .rows_affected();
            let reviews = sqlx::query_file!("src/llm/decisions/sql/outcomes/placement_review.sql")
                .execute(self.0.pool())
                .await?
                .rows_affected();
            Ok(verdicts + reviews)
        })
        .await
    }

    async fn sync_fleet_and_project_outcomes(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("sync_fleet_and_project_outcomes", async move {
            let triage = sqlx::query_file!("src/llm/decisions/sql/outcomes/triage_next_check.sql")
                .execute(self.0.pool())
                .await?
                .rows_affected();
            let merges =
                sqlx::query_file!("src/llm/decisions/sql/outcomes/project_merge_acted.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected();
            let threads =
                sqlx::query_file!("src/llm/decisions/sql/outcomes/thread_status_acted.sql")
                    .execute(self.0.pool())
                    .await?
                    .rows_affected();
            Ok(triage + merges + threads)
        })
        .await
    }
}
