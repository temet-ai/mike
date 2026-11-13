use chrono::{DateTime, Utc};
use sqlx::QueryBuilder;
use sqlx::postgres::Postgres;

use super::JudgeCandidateRow;
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn operator_turn_outcome_summary(
        &self,
        project: Option<&str>,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<(String, bool, i64)>, PersistError> {
        crate::metrics::time_db("operator_turn_outcome_summary", async move {
            let mut query: QueryBuilder<Postgres> = QueryBuilder::new(
                "SELECT o.code,
                        COALESCE(t.outcome_source = 'frustration_close', false) AS frustration_close,
                        COUNT(*)::bigint AS c
                   FROM memory.operator_turns t
                   JOIN projects.projects p ON p.id = t.project_id
                   JOIN memory.turn_outcomes o ON o.id = t.outcome_id
                  WHERE true",
            );
            if let Some(project_path) = project {
                query.push(" AND p.project_path = ").push_bind(project_path);
            }
            if let Some(since) = since {
                query.push(" AND t.created_at >= ").push_bind(since);
            }
            query.push(" GROUP BY 1, 2 ORDER BY 1 ASC, 2 ASC");
            let rows = query
                .build_query_as::<(String, bool, i64)>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn claim_unjudged_turns(
        &self,
        batch: i64,
        rubric_version: &str,
    ) -> Result<Vec<JudgeCandidateRow>, PersistError> {
        crate::metrics::time_db("claim_unjudged_turns", async move {
            let rows = sqlx::query_as!(
                JudgeCandidateRow,
                r#"SELECT id                                        AS turn_id,
                          COALESCE(user_prompt, '')           AS "user_prompt!",
                          COALESCE(final_assistant_text, '')  AS "final_assistant_text!",
                          COALESCE(tool_call_count, 0)::int   AS "tool_call_count!",
                          COALESCE(tool_error_count, 0)::int  AS "tool_error_count!"
                     FROM memory.operator_turns
                    WHERE closed_at   IS NOT NULL
                      AND outcome_id  = 1
                      AND (opened_by IS NULL OR opened_by = 'operator')
                      AND (judge_rubric_version IS NULL OR judge_rubric_version <> $1)
                      AND judge_failure_reason IS DISTINCT FROM 'permanent'
                      AND (judge_next_attempt_at IS NULL OR judge_next_attempt_at <= NOW())
                    ORDER BY closed_at DESC
                    LIMIT $2"#,
                rubric_version,
                batch,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
