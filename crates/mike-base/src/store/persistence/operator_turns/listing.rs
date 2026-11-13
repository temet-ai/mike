use sqlx::QueryBuilder;
use sqlx::postgres::Postgres;

use super::{
    ListTurnsFilter, OperatorTurnRow, QuiescenceTarget, QuiescentTurnRow, TURN_SELECT_BASE,
};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn list_quiescent_turns(
        &self,
        targets: &[QuiescenceTarget],
        limit: i64,
    ) -> Result<Vec<QuiescentTurnRow>, PersistError> {
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = targets.iter().map(|t| t.steerer_id).collect();
        let idle: Vec<i64> = targets.iter().map(|t| t.idle_after_secs).collect();
        let questions: Vec<Option<String>> = targets.iter().map(|t| t.question.clone()).collect();
        crate::metrics::time_db("list_quiescent_turns", async move {
            let rows = sqlx::query_file_as!(
                QuiescentTurnRow,
                "src/store/persistence/operator_turns/quiescent_turns.sql",
                &ids,
                &idle,
                &questions as _,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_turns(
        &self,
        filter: &ListTurnsFilter,
    ) -> Result<Vec<OperatorTurnRow>, PersistError> {
        if filter.correction && filter.outcome.is_some() {
            return Err(PersistError::DataInvariant(
                "list_turns: correction and outcome are mutually exclusive".to_owned(),
            ));
        }
        crate::metrics::time_db("list_operator_turns", async move {
            let limit = filter.limit.clamp(1, 500);
            let offset = filter.offset.max(0);
            let mut query: QueryBuilder<Postgres> = QueryBuilder::new(TURN_SELECT_BASE);
            query.push(" JOIN projects.projects p ON p.id = t.project_id WHERE true");
            if let Some(project_path) = filter.project_path.as_deref() {
                query
                    .push(" AND p.project_path ILIKE '%' || ")
                    .push_bind(project_path)
                    .push(" || '%'");
            }
            if let Some(session_db_id) = filter.session_db_id {
                query.push(" AND t.session_id = ").push_bind(session_db_id);
            }
            if let Some(since) = filter.since {
                query.push(" AND t.created_at >= ").push_bind(since);
            }
            if let Some(until) = filter.until {
                query.push(" AND t.created_at < ").push_bind(until);
            }
            if filter.correction {
                query
                    .push(" AND ")
                    .push(super::correction::COUNTS_AS_CORRECTION_SQL);
            } else if let Some(outcome) = filter.outcome.as_deref() {
                query.push(" AND toc.code = ").push_bind(outcome);
            }
            if let Some(subject_command) = filter.subject_command.as_deref() {
                query
                    .push(" AND t.subject_command = ")
                    .push_bind(subject_command);
            }
            if let Some(subject_agent) = filter.subject_agent.as_deref() {
                query
                    .push(" AND t.subject_agent = ")
                    .push_bind(subject_agent);
            }
            if let Some(skill) = filter.skill.as_deref() {
                query
                    .push(" AND ")
                    .push_bind(skill)
                    .push(" = ANY(t.skills_used)");
            }
            query
                .push(" ORDER BY t.created_at ")
                .push(filter.order.sql())
                .push(" LIMIT ")
                .push_bind(limit)
                .push(" OFFSET ")
                .push_bind(offset);
            let rows = query
                .build_query_as::<OperatorTurnRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }
}
