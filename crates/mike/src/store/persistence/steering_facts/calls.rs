use sqlx::{Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use super::{CallRow, FactKey, PersistedSteeringFacts, StaleFactTurn};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaleFilter {
    pub session_ids: Vec<i64>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: i64,
    pub after_turn: i64,
}

fn stale_query(f: &StaleFilter, version: i16) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(
        "SELECT f.operator_turn_id AS turn_id, f.session_id, t.closed_at
           FROM memory.turn_facts f
           JOIN memory.operator_turns t ON t.id = f.operator_turn_id
          WHERE t.closed_at IS NOT NULL",
    );
    if !f.session_ids.is_empty() {
        qb.push(" AND f.session_id = ANY(")
            .push_bind(&f.session_ids)
            .push(")");
    }
    qb.push(" AND (f.steering_facts_version < ")
        .push_bind(version);
    if let Some(from) = f.from {
        qb.push(" OR t.closed_at >= ").push_bind(from);
    }
    qb.push(")");
    if let Some(until) = f.until {
        qb.push(" AND t.closed_at < ").push_bind(until);
    }
    qb.push(" AND f.operator_turn_id > ")
        .push_bind(f.after_turn);
    qb.push(" ORDER BY f.operator_turn_id LIMIT ")
        .push_bind(f.limit);
    qb
}

impl PersistHandle {
    pub async fn steering_call_rows(&self, keys: &[FactKey]) -> Result<Vec<CallRow>, PersistError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let turns: Vec<i64> = keys.iter().map(|k| k.turn_id).collect();
        let at: Vec<chrono::DateTime<chrono::Utc>> = keys.iter().map(|k| k.as_of).collect();
        crate::metrics::time_db("steering_call_rows", async move {
            Ok(sqlx::query_file_as!(
                CallRow,
                "src/store/persistence/steering_facts/calls.sql",
                &turns,
                &at,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn persisted_steering_facts(
        &self,
        turn_id: i64,
    ) -> Result<Option<PersistedSteeringFacts>, PersistError> {
        crate::metrics::time_db("persisted_steering_facts", async move {
            Ok(sqlx::query_as!(
                PersistedSteeringFacts,
                "SELECT operator_turn_id, wait_backing, wait_backing_refs, observed_after_change,
                        last_change_use_id, last_change_at, last_observation_use_id,
                        last_observation_at, observation_kind, steering_facts_version,
                        session_class, session_class_rule
                   FROM memory.turn_facts WHERE operator_turn_id = $1",
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn stale_steering_fact_turns(
        &self,
        f: &StaleFilter,
        version: i16,
    ) -> Result<Vec<StaleFactTurn>, PersistError> {
        crate::metrics::time_db("stale_steering_fact_turns", async move {
            Ok(stale_query(f, version)
                .build_query_as::<StaleFactTurn>()
                .fetch_all(&self.pool)
                .await?)
        })
        .await
    }
}
