use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;
use super::{SemanticWindow, vector_literal};

const TURN_SOURCE_SELECT: &str =
    "SELECT t.id AS turn_id, t.session_id, t.project_id AS project_id, t.start_request_id AS request_id,
        COALESCE(t.user_prompt, '') AS user_prompt,
        COALESCE(t.final_assistant_text, '') AS final_text,
        f.text_spans, t.created_at, t.closed_at
   FROM memory.operator_turns t
   JOIN memory.turn_facts f ON f.operator_turn_id = t.id
  WHERE t.closed_at IS NOT NULL
    AND f.prompt_kind = 'genuine'
    AND f.session_class = 'operator'";

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct TurnSource {
    pub turn_id: i64,
    pub session_id: i64,
    pub project_id: Option<i64>,
    pub request_id: i64,
    pub user_prompt: String,
    pub final_text: String,
    pub text_spans: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveSource {
    pub entry_id: i64,
    pub session_id: i64,
    pub operator_turn_id: Option<i64>,
    pub project_id: Option<i64>,
    pub request_id: i64,
    pub prompt: String,
    pub data: serde_json::Value,
    pub effective_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NewItem {
    pub kind: &'static str,
    pub session_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub project_id: Option<i64>,
    pub request_id: Option<i64>,
    pub origin_session_uuid: Option<String>,
    pub origin_turn_id: Option<i64>,
    pub source_ref: String,
    pub pattern: Option<String>,
    pub text: String,
    pub embedding: Vec<f32>,
    pub embedder_key: String,
    pub vector_source: &'static str,
    pub builder_version: &'static str,
    pub effective_at: DateTime<Utc>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemanticItemRow {
    pub id: i64,
    pub kind: String,
    pub session_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub project_id: Option<i64>,
    pub request_id: Option<i64>,
    pub origin_session_uuid: Option<String>,
    pub origin_turn_id: Option<i64>,
    pub source_ref: String,
    pub pattern: Option<String>,
    pub text: String,
    pub text_chars: i32,
    pub embedding: String,
    pub embedder_key: String,
    pub vector_source: String,
    pub builder_version: String,
    pub effective_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemCount {
    pub embedder_key: String,
    pub kind: String,
    pub items: i64,
}

struct Columns {
    kind: Vec<&'static str>,
    session: Vec<Option<i64>>,
    turn: Vec<Option<i64>>,
    project: Vec<Option<i64>>,
    request: Vec<Option<i64>>,
    origin_session: Vec<Option<String>>,
    origin_turn: Vec<Option<i64>>,
    source_ref: Vec<String>,
    pattern: Vec<Option<String>>,
    text: Vec<String>,
    vec: Vec<String>,
    key: Vec<String>,
    source: Vec<&'static str>,
    version: Vec<&'static str>,
    effective: Vec<DateTime<Utc>>,
    data: Vec<serde_json::Value>,
}

fn columns(items: &[NewItem]) -> Columns {
    Columns {
        kind: items.iter().map(|i| i.kind).collect(),
        session: items.iter().map(|i| i.session_id).collect(),
        turn: items.iter().map(|i| i.operator_turn_id).collect(),
        project: items.iter().map(|i| i.project_id).collect(),
        request: items.iter().map(|i| i.request_id).collect(),
        origin_session: items
            .iter()
            .map(|i| i.origin_session_uuid.clone())
            .collect(),
        origin_turn: items.iter().map(|i| i.origin_turn_id).collect(),
        source_ref: items.iter().map(|i| i.source_ref.clone()).collect(),
        pattern: items.iter().map(|i| i.pattern.clone()).collect(),
        text: items.iter().map(|i| i.text.clone()).collect(),
        vec: items.iter().map(|i| vector_literal(&i.embedding)).collect(),
        key: items.iter().map(|i| i.embedder_key.clone()).collect(),
        source: items.iter().map(|i| i.vector_source).collect(),
        version: items.iter().map(|i| i.builder_version).collect(),
        effective: items.iter().map(|i| i.effective_at).collect(),
        data: items.iter().map(|i| i.data.clone()).collect(),
    }
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn semantic_request_prompts(
        self,
        ids: &[i64],
    ) -> Result<Vec<(i64, String)>, PersistError> {
        crate::metrics::time_db("semantic_request_prompts", async move {
            let rows = sqlx::query!(
                "SELECT id, human_prompt FROM requests.requests
                  WHERE id = ANY($1) AND char_length(human_prompt) > 0",
                ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| (r.id, r.human_prompt)).collect())
        })
        .await
    }

    pub async fn turns_needing_semantic_items(
        self,
        embedder_key: &str,
        builder_version: &str,
        window: &SemanticWindow,
    ) -> Result<Vec<TurnSource>, PersistError> {
        crate::metrics::time_db("turns_needing_semantic_items", async move {
            let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(TURN_SOURCE_SELECT);
            if let Some(from) = window.from {
                query.push(" AND t.closed_at >= ").push_bind(from);
            }
            if let Some(until) = window.until {
                query.push(" AND t.closed_at < ").push_bind(until);
            }
            query.push(" AND t.id > ").push_bind(window.after_turn);
            query.push(
                " AND NOT EXISTS (
        SELECT 1 FROM memory.semantic_items i
         WHERE i.operator_turn_id = t.id AND i.kind IN ('turn_prompt', 'turn_ending')
           AND i.embedder_key = ",
            );
            query.push_bind(embedder_key);
            query
                .push(" AND i.builder_version = ")
                .push_bind(builder_version);
            query.push(") ORDER BY t.id LIMIT ").push_bind(window.limit);
            Ok(query
                .build_query_as::<TurnSource>()
                .fetch_all(&self.pool)
                .await?)
        })
        .await
    }

    pub async fn semantic_turn_source(
        self,
        turn_id: i64,
    ) -> Result<Option<TurnSource>, PersistError> {
        crate::metrics::time_db("semantic_turn_source", async move {
            Ok(sqlx::query_as!(
                TurnSource,
                r#"SELECT t.id AS turn_id, t.session_id, t.project_id AS "project_id?",
                          t.start_request_id AS request_id,
                          COALESCE(t.user_prompt, '') AS "user_prompt!",
                          COALESCE(t.final_assistant_text, '') AS "final_text!",
                          f.text_spans, t.created_at, t.closed_at
                     FROM memory.operator_turns t
                     JOIN memory.turn_facts f ON f.operator_turn_id = t.id
                    WHERE t.id = $1 AND t.closed_at IS NOT NULL"#,
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn copy_semantic_prompt_items(
        self,
        turn_ids: &[i64],
        embedder_key: &str,
        builder_version: &str,
        model: &str,
        copy_from: DateTime<Utc>,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("copy_semantic_prompt_items", async move {
            Ok(sqlx::query_scalar!(
                r#"INSERT INTO memory.semantic_items
                       (kind, session_id, operator_turn_id, project_id, request_id, source_ref, text,
                        text_chars, embedding, embedder_key, vector_source, builder_version,
                        effective_at)
                   SELECT 'turn_prompt', t.session_id, t.id, t.project_id, t.start_request_id,
                          'memory.operator_turns:' || t.id, c.chunk_text,
                          char_length(c.chunk_text), c.chunk_embedding, $2, 'chunk_copy', $3,
                          t.created_at
                     FROM memory.operator_turns t
                     JOIN memory.operator_turn_chunks c ON c.turn_id = t.id AND c.chunk_idx = 0
                     JOIN memory.chunk_roles cr ON cr.id = c.chunk_role_id AND cr.code = 'prompt'
                    WHERE t.id = ANY($1)
                      AND c.embedding_model = $4
                      AND c.embedded_at >= $5
                      AND char_length(c.chunk_text) > 0
                   ON CONFLICT (kind, source_ref, embedder_key, builder_version) DO NOTHING
                   RETURNING operator_turn_id AS "operator_turn_id!""#,
                turn_ids,
                embedder_key,
                builder_version,
                model,
                copy_from,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn insert_semantic_items(self, items: &[NewItem]) -> Result<u64, PersistError> {
        if items.is_empty() {
            return Ok(0);
        }
        let c = columns(items);
        crate::metrics::time_db("insert_semantic_items", async move {
            let r = sqlx::query!(
                "INSERT INTO memory.semantic_items
                     (kind, session_id, operator_turn_id, project_id, request_id,
                      origin_session_uuid, origin_turn_id, source_ref, pattern, text, text_chars,
                      embedding, embedder_key, vector_source, builder_version, effective_at, data)
                 SELECT u.kind, u.session_id, u.turn_id, u.project_id, u.request_id,
                        u.origin_session::uuid, u.origin_turn, u.source_ref, u.pattern, u.text,
                        char_length(u.text), u.vec::vector, u.embedder_key, u.vector_source,
                        u.builder_version, u.effective_at, u.data
                   FROM unnest($1::text[], $2::bigint[], $3::bigint[], $4::bigint[], $5::bigint[],
                               $6::text[], $7::bigint[], $8::text[], $9::text[], $10::text[],
                               $11::text[], $12::text[], $13::text[], $14::text[],
                               $15::timestamptz[], $16::jsonb[])
                        AS u(kind, session_id, turn_id, project_id, request_id, origin_session,
                             origin_turn, source_ref, pattern, text, vec, embedder_key,
                             vector_source, builder_version, effective_at, data)
                 ON CONFLICT (kind, source_ref, embedder_key, builder_version) DO NOTHING",
                &c.kind as &[&str],
                &c.session as &[Option<i64>],
                &c.turn as &[Option<i64>],
                &c.project as &[Option<i64>],
                &c.request as &[Option<i64>],
                &c.origin_session as &[Option<String>],
                &c.origin_turn as &[Option<i64>],
                &c.source_ref,
                &c.pattern as &[Option<String>],
                &c.text,
                &c.vec,
                &c.key,
                &c.source as &[&str],
                &c.version as &[&str],
                &c.effective,
                &c.data,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected())
        })
        .await
    }

    pub async fn directives_needing_semantic_items(
        self,
        embedder_key: &str,
        builder_version: &str,
        limit: i64,
    ) -> Result<Vec<DirectiveSource>, PersistError> {
        crate::metrics::time_db("directives_needing_semantic_items", async move {
            Ok(sqlx::query_as!(
                DirectiveSource,
                r#"SELECT e.id AS entry_id, e.session_id, e.operator_turn_id,
                          s.project_id AS "project_id?",
                          split_part(e.source_ref, ':', 2)::bigint AS "request_id!",
                          COALESCE(r.human_prompt, '') AS "prompt!", e.data, e.effective_at
                     FROM memory.dossier_entries e
                     JOIN sessions.sessions s ON s.id = e.session_id
                     JOIN requests.requests r ON r.id = split_part(e.source_ref, ':', 2)::bigint
                    WHERE e.kind = 'directive'
                      AND e.source IN ('capture', 'operator')
                      AND e.source_ref LIKE 'requests.requests:%'
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.semantic_items i
                           WHERE i.kind = 'directive'
                             AND i.source_ref = 'memory.dossier_entries:' || e.id
                             AND i.embedder_key = $1 AND i.builder_version = $2)
                    ORDER BY e.id
                    LIMIT $3"#,
                embedder_key,
                builder_version,
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn get_semantic_item(self, id: i64) -> Result<Option<SemanticItemRow>, PersistError> {
        crate::metrics::time_db("get_semantic_item", async move {
            Ok(sqlx::query_as!(
                SemanticItemRow,
                r#"SELECT id, kind, session_id, operator_turn_id, project_id, request_id,
                          origin_session_uuid::text AS origin_session_uuid, origin_turn_id,
                          source_ref, pattern, text, text_chars,
                          embedding::text AS "embedding!", embedder_key, vector_source,
                          builder_version, effective_at, recorded_at, data
                     FROM memory.semantic_items WHERE id = $1"#,
                id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn semantic_item_counts(self) -> Result<Vec<ItemCount>, PersistError> {
        crate::metrics::time_db("semantic_item_counts", async move {
            Ok(sqlx::query_as!(
                ItemCount,
                r#"SELECT embedder_key, kind, count(*)::bigint AS "items!"
                     FROM memory.semantic_items
                    GROUP BY embedder_key, kind
                    ORDER BY embedder_key, kind"#,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn semantic_backlog(self, embedder_key: &str) -> Result<i64, PersistError> {
        crate::metrics::time_db("semantic_backlog", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT count(*)::bigint AS "count!"
                     FROM memory.operator_turns t
                     JOIN memory.turn_facts f ON f.operator_turn_id = t.id
                    WHERE t.closed_at IS NOT NULL AND f.prompt_kind = 'genuine'
                      AND f.session_class = 'operator'
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.semantic_items i
                           WHERE i.operator_turn_id = t.id
                             AND i.kind IN ('turn_prompt', 'turn_ending')
                             AND i.embedder_key = $1)"#,
                embedder_key,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
