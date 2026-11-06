use super::{PersistError, PersistHandle};
use sqlx::{Postgres, QueryBuilder};

#[derive(sqlx::FromRow)]
pub(crate) struct RankedTurnRow {
    pub(crate) turn_id: i64,
    pub(crate) session_id: String,
    pub(crate) project_id: i64,
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) user_prompt: String,
    pub(crate) final_assistant_text: String,
    pub(crate) tool_call_count: i32,
    pub(crate) tool_error_count: i32,
    pub(crate) files_touched: Vec<String>,
    pub(crate) tools_used: Vec<String>,
    pub(crate) agents_dispatched: Vec<String>,
    pub(crate) weasel_findings_count: i32,
    pub(crate) total_input_tokens: i64,
    pub(crate) total_output_tokens: i64,
    pub(crate) duration_ms: i64,
    pub(crate) interrupted: bool,
    pub(crate) outcome: String,
    pub(crate) outcome_evidence: Option<String>,
    pub(crate) classified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) rank: f32,
}

const TURN_COLUMNS: &str =
    "t.turn_id, t.session_id, t.project_id, t.start_request_id, t.end_request_id,
                        t.user_prompt, t.final_assistant_text,
                        t.tool_call_count, t.tool_error_count,
                        t.files_touched, t.tools_used, t.agents_dispatched,
                        t.weasel_findings_count,
                        t.total_input_tokens, t.total_output_tokens, t.duration_ms,
                        t.interrupted, t.outcome, t.outcome_evidence, t.classified_at,
                        t.created_at, t.updated_at";

fn by_file_query(project_path: &str, file: &str, limit: i64) -> QueryBuilder<Postgres> {
    let mut query = QueryBuilder::new(crate::store::operator_turns::TURN_SELECT_BASE);
    query
        .push(
            "
       JOIN projects.projects p ON p.id = t.project_id
      WHERE p.project_path = ",
        )
        .push_bind(project_path)
        .push(
            "
        AND (t.opened_by IS NULL OR t.opened_by = 'operator')
        AND EXISTS (
            SELECT 1
              FROM mike.file_touches_v ft
              JOIN requests.requests r
                ON r.id = ft.request_id
             WHERE ft.file_path = ",
        )
        .push_bind(file)
        .push(
            "
               AND ft.request_id BETWEEN t.start_request_id AND t.end_request_id
               AND r.session_id = s.id
        )
      ORDER BY t.created_at DESC
      LIMIT ",
        )
        .push_bind(limit);
    query
}

fn by_embedding_query(
    project_path: &str,
    query_vec_literal: &str,
    limit: i64,
) -> QueryBuilder<Postgres> {
    let mut query = QueryBuilder::new(
        "WITH chunk_distances AS (
            SELECT
                c.turn_id,
                MIN(c.chunk_embedding <=> ",
    );
    query
        .push_bind(query_vec_literal)
        .push(
            "::vector) AS min_distance
            FROM memory.operator_turn_chunks c
            JOIN memory.operator_turns ot ON c.turn_id = ot.id
            JOIN projects.projects p ON p.id = ot.project_id
            WHERE p.project_path = ",
        )
        .push_bind(project_path)
        .push(
            "
              AND (ot.opened_by IS NULL OR ot.opened_by = 'operator')
            GROUP BY c.turn_id
         ),
         ranked AS (
            SELECT turn_id, min_distance
              FROM chunk_distances
             ORDER BY min_distance ASC
             LIMIT ",
        )
        .push_bind(limit)
        .push(
            "
         )
         SELECT ",
        )
        .push(TURN_COLUMNS)
        .push(
            ",
                (1.0 - ranked.min_distance)::real AS rank
           FROM ranked
           JOIN (",
        )
        .push(crate::store::operator_turns::TURN_SELECT_BASE)
        .push(
            ") t ON t.turn_id = ranked.turn_id
          ORDER BY ranked.min_distance ASC, t.created_at DESC",
        );
    query
}

const FTS_TEXT_VECTOR: &str = "to_tsvector('english', COALESCE(tx.user_prompt, '') || ' ' || COALESCE(tx.final_assistant_text, ''))";

fn by_fts_query(project_path: &str, text_query: &str, limit: i64) -> QueryBuilder<Postgres> {
    let mut query = QueryBuilder::new("WITH q AS (SELECT websearch_to_tsquery('english', ");
    query
        .push_bind(text_query)
        .push(
            ") AS tsq
         ),
         fast AS (
            SELECT t.id AS turn_id,
                   ts_rank_cd(t.search_tsv, q.tsq)::real AS rank
              FROM memory.operator_turns t
              JOIN projects.projects p ON p.id = t.project_id
              CROSS JOIN q
             WHERE p.project_path = ",
        )
        .push_bind(project_path)
        .push(
            "
               AND (t.opened_by IS NULL OR t.opened_by = 'operator')
               AND t.search_tsv @@ q.tsq
         ),
         exception AS (
            SELECT t.id AS turn_id,
                   ts_rank_cd(",
        )
        .push(FTS_TEXT_VECTOR)
        .push(
            ", q.tsq)::real AS rank
              FROM memory.operator_turns t
              JOIN projects.projects p ON p.id = t.project_id
              JOIN mike.operator_turn_text_v tx ON tx.turn_id = t.id
              CROSS JOIN q
             WHERE p.project_path = ",
        )
        .push_bind(project_path)
        .push(
            "
               AND (t.opened_by IS NULL OR t.opened_by = 'operator')
               AND (t.user_prompt IS NULL OR t.closed_at IS NULL)
               AND ",
        )
        .push(FTS_TEXT_VECTOR)
        .push(
            " @@ q.tsq
         ),
         candidates AS (
            SELECT turn_id, rank FROM fast
            UNION ALL
            SELECT turn_id, rank FROM exception
         ),
         ranked AS (
            SELECT turn_id, MAX(rank) AS rank
              FROM candidates
             GROUP BY turn_id
             ORDER BY rank DESC
             LIMIT ",
        )
        .push_bind(limit)
        .push(
            "
         )
         SELECT ",
        )
        .push(TURN_COLUMNS)
        .push(
            ",
                ranked.rank
           FROM ranked
           JOIN (",
        )
        .push(crate::store::operator_turns::TURN_SELECT_BASE)
        .push(
            ") t ON t.turn_id = ranked.turn_id
          ORDER BY ranked.rank DESC, t.created_at DESC",
        );
    query
}

impl PersistHandle {
    pub(crate) async fn recall_turns_by_file(
        &self,
        project_path: &str,
        file: &str,
        limit: i64,
    ) -> Result<Vec<crate::store::operator_turns::OperatorTurnRow>, PersistError> {
        let project_path = self
            .resolve_knowledge_project(project_path)
            .await?
            .project_path;
        crate::metrics::time_db("recall_turns_by_file", async {
            let rows = by_file_query(&project_path, file, limit)
                .build_query_as::<crate::store::operator_turns::OperatorTurnRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn recall_turns_by_embedding(
        &self,
        project_path: &str,
        query_vec_literal: &str,
        limit: i64,
    ) -> Result<Option<Vec<RankedTurnRow>>, PersistError> {
        let project_path = self
            .resolve_knowledge_project(project_path)
            .await?
            .project_path;
        crate::metrics::time_db("recall_turns_by_embedding", async {
            let mut tx = self.pool.begin().await?;
            sqlx::query!("SET LOCAL jit = off")
                .execute(&mut *tx)
                .await?;
            sqlx::query!("SET LOCAL statement_timeout = '12s'")
                .execute(&mut *tx)
                .await?;
            let rows = by_embedding_query(&project_path, query_vec_literal, limit)
                .build_query_as::<RankedTurnRow>()
                .fetch_all(&mut *tx)
                .await?;
            tx.commit().await?;
            if rows.is_empty() {
                Ok(None)
            } else {
                Ok(Some(rows))
            }
        })
        .await
    }

    pub(crate) async fn recall_turns_by_fts(
        &self,
        project_path: &str,
        query: &str,
        limit: i64,
    ) -> Result<Vec<RankedTurnRow>, PersistError> {
        let project_path = self
            .resolve_knowledge_project(project_path)
            .await?
            .project_path;
        crate::metrics::time_db("recall_turns_by_fts", async {
            let mut tx = self.pool.begin().await?;
            sqlx::query!("SET LOCAL jit = off")
                .execute(&mut *tx)
                .await?;
            sqlx::query!("SET LOCAL statement_timeout = '12s'")
                .execute(&mut *tx)
                .await?;
            let rows = by_fts_query(&project_path, query, limit)
                .build_query_as::<RankedTurnRow>()
                .fetch_all(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(rows)
        })
        .await
    }
}
