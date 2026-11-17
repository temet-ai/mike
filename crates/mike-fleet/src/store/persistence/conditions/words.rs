use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::types::ConditionError;
use crate::store::persistence::fleet_repo::FleetDb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordsGate {
    pub session_id: i64,
    pub turn_landed: bool,
}

impl FleetDb<'_> {
    pub async fn words_gate(
        self,
        condition_id: i64,
        session_uuid: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Option<WordsGate>, ConditionError> {
        let row = sqlx::query_file!(
            "src/store/persistence/conditions/sql/words_turn_landed.sql",
            condition_id,
            session_uuid,
            since
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| WordsGate {
            session_id: r.session_id,
            turn_landed: r.landed,
        }))
    }

    pub async fn stamp_condition_checked(self, condition_id: i64) -> Result<(), ConditionError> {
        sqlx::query_file!(
            "src/store/persistence/conditions/sql/stamp_checked.sql",
            condition_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_condition_judged(
        self,
        condition_id: i64,
        through: DateTime<Utc>,
    ) -> Result<(), ConditionError> {
        sqlx::query_file!(
            "src/store/persistence/conditions/sql/mark_judged.sql",
            condition_id,
            through
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_condition_asked(self, condition_id: i64) -> Result<bool, ConditionError> {
        let row = sqlx::query_file!(
            "src/store/persistence/conditions/sql/mark_asked.sql",
            condition_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }
}
