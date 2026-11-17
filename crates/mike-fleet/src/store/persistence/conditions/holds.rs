use chrono::{DateTime, Duration, Utc};

use super::spec::{FleetEventSpec, SessionSpec, Spec, SqlExpect, SqlSpec};
use super::types::{ConditionError, ConditionRow};
use crate::store::persistence::fleet_repo::FleetDb;

const SQL_TIMEOUT: &str = "SET LOCAL statement_timeout = '2s'";

pub fn sql_check_due(row: &ConditionRow, now: DateTime<Utc>) -> bool {
    row.last_checked_at
        .is_none_or(|at| now - at >= Duration::seconds(i64::from(row.check_every_secs)))
}

pub fn expectation_met(expect: SqlExpect, any_rows: bool) -> bool {
    match expect {
        SqlExpect::Rows => any_rows,
        SqlExpect::NoRows => !any_rows,
    }
}

impl FleetDb<'_> {
    pub async fn condition_holds(
        self,
        row: &ConditionRow,
        now: DateTime<Utc>,
    ) -> Result<bool, ConditionError> {
        let spec = Spec::parse(&row.kind, &row.spec)?;
        let since = spec.since().unwrap_or(row.not_before);
        match spec {
            Spec::Time => Ok(now >= row.not_before),
            Spec::SessionTurnEnded(s) => self.session_turn_ended(&s, since).await,
            Spec::SessionEnded(s) => self.session_ended(&s, since).await,
            Spec::FleetEvent(s) => self.fleet_event_seen(&s, since).await,
            Spec::Sql(s) => self.sql_holds(row, &s, now).await,
            Spec::Words(_) => Err(ConditionError::Invalid(
                "a words condition is judged by the side-call chain in the conditions worker, not by the store".to_owned(),
            )),
        }
    }

    async fn session_turn_ended(
        self,
        spec: &SessionSpec,
        since: DateTime<Utc>,
    ) -> Result<bool, ConditionError> {
        let found = sqlx::query_file!(
            "src/store/persistence/conditions/sql/turn_ended.sql",
            spec.session_uuid,
            since
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(found.holds)
    }

    async fn session_ended(
        self,
        spec: &SessionSpec,
        since: DateTime<Utc>,
    ) -> Result<bool, ConditionError> {
        let found = sqlx::query_file!(
            "src/store/persistence/conditions/sql/session_ended.sql",
            spec.session_uuid,
            since
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(found.holds)
    }

    async fn fleet_event_seen(
        self,
        spec: &FleetEventSpec,
        since: DateTime<Utc>,
    ) -> Result<bool, ConditionError> {
        let found = sqlx::query_file!(
            "src/store/persistence/conditions/sql/fleet_event.sql",
            spec.run_id,
            spec.kind,
            spec.task,
            spec.session_uuid,
            since
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(found.holds)
    }

    async fn sql_holds(
        self,
        row: &ConditionRow,
        spec: &SqlSpec,
        now: DateTime<Utc>,
    ) -> Result<bool, ConditionError> {
        if !sql_check_due(row, now) {
            return Ok(false);
        }
        sqlx::query_file!(
            "src/store/persistence/conditions/sql/stamp_checked.sql",
            row.id
        )
        .execute(&self.pool)
        .await?;
        let any_rows = self.run_read_only_exists(&spec.query).await.map_err(|e| {
            ConditionError::Invalid(format!(
                "sql condition query failed: {e}; query was {}",
                spec.query
            ))
        })?;
        Ok(expectation_met(spec.expect, any_rows))
    }

    pub async fn run_read_only_exists(self, query: &str) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION READ ONLY") // grader-allow: a transaction setting with no result, the guard for the runtime sql condition below
            .execute(&mut *tx)
            .await?;
        sqlx::query(SQL_TIMEOUT).execute(&mut *tx).await?; // grader-allow: SET LOCAL statement_timeout, the 2 s guard for the runtime sql condition below
        let wrapped = format!("SELECT EXISTS (\n{query}\n)");
        sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(wrapped)) // nosemgrep: mike.rust.sql-from-string grader-allow: a sql condition is a query the registering session supplies at runtime; it runs in a READ ONLY transaction with a 2 s timeout, and insert refuses an empty query, a `;` or more than 4000 characters
            .fetch_one(&mut *tx)
            .await
    }
}
