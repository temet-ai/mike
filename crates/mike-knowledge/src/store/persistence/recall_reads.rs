use super::PersistError;
use super::project_knowledge::ProjectIdentity;
use super::recall_embedding::{by_embedding_query, set_recall_timeouts, set_vector_scan_limits};
use sqlx::{Postgres, QueryBuilder};

#[derive(sqlx::FromRow)]
pub struct RankedTurnRow {
    pub turn_id: i64,
    pub session_id: String,
    pub project_id: i64,
    pub start_request_id: i64,
    pub end_request_id: i64,
    pub user_prompt: String,
    pub final_assistant_text: String,
    pub tool_call_count: i32,
    pub tool_error_count: i32,
    pub files_touched: Vec<String>,
    pub tools_used: Vec<String>,
    pub agents_dispatched: Vec<String>,
    pub weasel_findings_count: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub duration_ms: i64,
    pub interrupted: bool,
    pub outcome: String,
    pub outcome_evidence: Option<String>,
    pub classified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub rank: f32,
}

pub const TURN_COLUMNS: &str =
    "t.turn_id, t.session_id, t.project_id, t.start_request_id, t.end_request_id,
                        t.user_prompt, t.final_assistant_text,
                        t.tool_call_count, t.tool_error_count,
                        t.files_touched, t.tools_used, t.agents_dispatched,
                        t.weasel_findings_count,
                        t.total_input_tokens, t.total_output_tokens, t.duration_ms,
                        t.interrupted, t.outcome, t.outcome_evidence, t.classified_at,
                        t.created_at, t.updated_at";

fn by_file_query(project_id: i64, file: &str, limit: i64) -> QueryBuilder<Postgres> {
    let mut query = QueryBuilder::new(
        "WITH picked AS (
            SELECT t.id AS turn_id, t.created_at
              FROM memory.operator_turns t
              JOIN sessions.sessions s ON s.id = t.session_id
             WHERE t.project_id = ",
    );
    query
        .push_bind(project_id)
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
        .push_bind(limit)
        .push(
            "
         )
         SELECT t.*
           FROM picked
          CROSS JOIN LATERAL (",
        )
        .push(crate::store::operator_turns::TURN_SELECT_BASE)
        .push(
            "
             WHERE t.id = picked.turn_id
             OFFSET 0) t
          ORDER BY t.created_at DESC",
        );
    query
}

const FTS_TEXT_VECTOR: &str = "to_tsvector('english', COALESCE(tx.user_prompt, '') || ' ' || COALESCE(tx.final_assistant_text, ''))";

fn by_fts_query(
    project_id: i64,
    text_query: &str,
    limit: i64,
    exclude_turn: Option<i64>,
) -> QueryBuilder<Postgres> {
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
              CROSS JOIN q
             WHERE t.project_id = ",
        )
        .push_bind(project_id)
        .push(
            "
               AND (t.opened_by IS NULL OR t.opened_by = 'operator')
               AND t.search_tsv @@ q.tsq
               AND t.id IS DISTINCT FROM ",
        )
        .push_bind(exclude_turn)
        .push(
            "
         ),
         exception AS (
            SELECT t.id AS turn_id,
                   ts_rank_cd(",
        )
        .push(FTS_TEXT_VECTOR)
        .push(
            ", q.tsq)::real AS rank
              FROM memory.operator_turns t
              JOIN mike.operator_turn_text_v tx ON tx.turn_id = t.id
              CROSS JOIN q
             WHERE t.project_id = ",
        )
        .push_bind(project_id)
        .push(
            "
               AND (t.opened_by IS NULL OR t.opened_by = 'operator')
               AND (t.user_prompt IS NULL OR t.closed_at IS NULL)
               AND ",
        )
        .push(FTS_TEXT_VECTOR)
        .push(
            " @@ q.tsq
               AND t.id IS DISTINCT FROM ",
        )
        .push_bind(exclude_turn)
        .push(
            "
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
          CROSS JOIN LATERAL (",
        )
        .push(crate::store::operator_turns::TURN_SELECT_BASE)
        .push(
            "
             WHERE t.id = ranked.turn_id
             OFFSET 0) t
          ORDER BY ranked.rank DESC, t.created_at DESC",
        );
    query
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn newest_turn_of_session(
        self,
        session_uuid: &str,
    ) -> Result<Option<i64>, PersistError> {
        let Ok(session) = uuid::Uuid::parse_str(session_uuid.trim()) else {
            return Ok(None);
        };
        crate::metrics::time_db("newest_turn_of_session", async {
            Ok(sqlx::query_scalar!(
                "SELECT max(t.id) FROM memory.operator_turns t
               JOIN sessions.sessions s ON s.id = t.session_id
              WHERE s.session_uuid = $1 AND (t.opened_by IS NULL OR t.opened_by = 'operator')",
                session
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn recall_turns_by_file(
        self,
        project: &ProjectIdentity,
        file: &str,
        limit: i64,
    ) -> Result<Vec<crate::store::operator_turns::OperatorTurnRow>, PersistError> {
        crate::metrics::time_db("recall_turns_by_file", async {
            let rows = by_file_query(project.id, file, limit)
                .build_query_as::<crate::store::operator_turns::OperatorTurnRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn recall_turns_by_embedding(
        self,
        project: &ProjectIdentity,
        embedding_model: &str,
        query_vec_literal: &str,
        limit: i64,
        exclude_turn: Option<i64>,
    ) -> Result<Option<Vec<RankedTurnRow>>, PersistError> {
        crate::metrics::time_db("recall_turns_by_embedding", async {
            let mut tx = self.pool.begin().await?;
            set_recall_timeouts(&mut tx).await?;
            set_vector_scan_limits(&mut tx).await?;
            let search = (embedding_model, query_vec_literal);
            let rows = by_embedding_query(project.id, search, limit, exclude_turn)
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

    pub async fn recall_turns_by_fts(
        self,
        project: &ProjectIdentity,
        query: &str,
        limit: i64,
        exclude_turn: Option<i64>,
    ) -> Result<Vec<RankedTurnRow>, PersistError> {
        crate::metrics::time_db("recall_turns_by_fts", async {
            let mut tx = self.pool.begin().await?;
            set_recall_timeouts(&mut tx).await?;
            let rows = by_fts_query(project.id, query, limit, exclude_turn)
                .build_query_as::<RankedTurnRow>()
                .fetch_all(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(rows)
        })
        .await
    }
}
