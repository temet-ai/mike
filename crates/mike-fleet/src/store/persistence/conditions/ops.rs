use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::spec::validate_new;
use super::types::{
    ConditionError, ConditionFilter, ConditionRow, Fired, NewCondition, SessionPresence,
};
use crate::store::persistence::fleet_repo::FleetDb;

const LIST_LIMIT_MAX: i64 = 200;
const DUE_LIMIT: i64 = 20;

impl FleetDb<'_> {
    pub async fn insert_condition(
        self,
        new: &NewCondition,
    ) -> Result<ConditionRow, ConditionError> {
        validate_new(new)?;
        let row = sqlx::query_file_as!(
            ConditionRow,
            "src/store/persistence/conditions/sql/insert.sql",
            new.machine_uuid,
            new.created_by_session,
            new.project_id,
            new.thread_slug,
            new.kind,
            new.spec,
            new.not_before,
            new.deadline,
            new.check_every_secs,
            new.action,
            new.target_session,
            new.folder,
            new.brief
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_conditions(
        self,
        filter: &ConditionFilter,
    ) -> Result<Vec<ConditionRow>, ConditionError> {
        let limit = filter.limit.clamp(1, LIST_LIMIT_MAX);
        let rows = sqlx::query_file_as!(
            ConditionRow,
            "src/store/persistence/conditions/sql/list.sql",
            filter.thread_slug,
            filter.state,
            filter.created_by_session,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn cancel_condition(self, condition_uuid: Uuid) -> Result<bool, ConditionError> {
        let done = sqlx::query_file!(
            "src/store/persistence/conditions/sql/cancel.sql",
            condition_uuid
        )
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    pub async fn due_conditions(
        self,
        machine_uuid: Uuid,
    ) -> Result<Vec<ConditionRow>, ConditionError> {
        let rows = sqlx::query_file_as!(
            ConditionRow,
            "src/store/persistence/conditions/sql/due.sql",
            machine_uuid,
            DUE_LIMIT
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn next_armed_condition_at(
        self,
        machine_uuid: Uuid,
    ) -> Result<Option<DateTime<Utc>>, ConditionError> {
        let at = sqlx::query_file_scalar!(
            "src/store/persistence/conditions/sql/next_armed.sql",
            machine_uuid
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(at)
    }

    pub async fn mark_condition_fired(
        self,
        id: i64,
        fired: Fired,
        outcome: &Value,
    ) -> Result<Option<ConditionRow>, ConditionError> {
        let row = sqlx::query_file_as!(
            ConditionRow,
            "src/store/persistence/conditions/sql/mark_fired.sql",
            id,
            fired.state(),
            outcome
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn set_condition_outcome(
        self,
        id: i64,
        outcome: &Value,
    ) -> Result<(), ConditionError> {
        sqlx::query_file!(
            "src/store/persistence/conditions/sql/set_outcome.sql",
            id,
            outcome
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_condition_failed_attempt(
        self,
        id: i64,
        error: &str,
    ) -> Result<Option<ConditionRow>, ConditionError> {
        let row = sqlx::query_file_as!(
            ConditionRow,
            "src/store/persistence/conditions/sql/mark_failed_attempt.sql",
            id,
            error
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn condition_session_presence(
        self,
        session_uuid: Uuid,
    ) -> Result<Option<SessionPresence>, ConditionError> {
        let row = sqlx::query_file!(
            "src/store/persistence/conditions/sql/presence.sql",
            session_uuid
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| SessionPresence {
            session_uuid: r.session_uuid,
            project_id: r.project_id,
            last_request_at: r.last_request_at,
            cwd: r.cwd,
            live: r.live,
        }))
    }
}
