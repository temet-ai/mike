use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use super::{EvalLabelListRow, ImportBatchRow};

#[derive(Debug, Clone, Default)]
pub struct LabelQuery {
    pub subject_kind: Option<String>,
    pub turn_id: Option<i64>,
    pub session_uuid: Option<uuid::Uuid>,
    pub axis: Option<String>,
    pub tiers: Option<Vec<String>>,
    pub method: Option<String>,
    pub split: Option<String>,
    pub source: Option<String>,
    pub current_only: bool,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub before_id: Option<i64>,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct VocabularyRow {
    pub axis: String,
    pub label: String,
    pub subject_kinds: Vec<String>,
    pub sort_order: i32,
    pub description: String,
}

const LIST_SELECT: &str = "SELECT l.id, l.subject_kind, l.subject_id, l.operator_turn_id,
        l.session_id, l.session_uuid, l.axis, l.label, l.action, l.tier, l.method, l.split, l.tags,
        l.severity, l.confidence, l.evidence, l.note, l.source, l.provenance, l.inclusion_prob,
        l.verdict_snapshot, l.verdict_bundle_id, l.import_batch_id, l.supersedes_id, l.created_at,
        ts.split_reason, (cur.id IS NOT NULL) AS is_current,
        EXISTS (SELECT 1 FROM memory.operator_turns t WHERE t.id = l.operator_turn_id) AS subject_present
   FROM eval.labels l
   LEFT JOIN eval.turn_splits ts ON ts.operator_turn_id = l.operator_turn_id
   LEFT JOIN eval.labels_current_v cur ON cur.id = l.id
  WHERE l.id < ";

fn list_labels_query(query: &LabelQuery) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(LIST_SELECT);
    qb.push_bind(query.before_id.unwrap_or(i64::MAX));
    if let Some(kind) = &query.subject_kind {
        qb.push(" AND l.subject_kind = ").push_bind(kind);
    }
    if let Some(turn_id) = query.turn_id {
        qb.push(" AND l.operator_turn_id = ").push_bind(turn_id);
    }
    if let Some(session_uuid) = query.session_uuid {
        qb.push(" AND l.session_uuid = ").push_bind(session_uuid);
    }
    if let Some(axis) = &query.axis {
        qb.push(" AND l.axis = ").push_bind(axis);
    }
    if let Some(tiers) = &query.tiers {
        qb.push(" AND l.tier = ANY(").push_bind(tiers).push(")");
    }
    if let Some(method) = &query.method {
        qb.push(" AND l.method = ").push_bind(method);
    }
    if let Some(split) = &query.split {
        qb.push(" AND l.split = ").push_bind(split);
    }
    if let Some(source) = &query.source {
        qb.push(" AND l.source = ").push_bind(source);
    }
    if query.current_only {
        qb.push(" AND cur.id IS NOT NULL");
    }
    if let Some(since) = query.since {
        qb.push(" AND l.created_at >= ").push_bind(since);
    }
    if let Some(until) = query.until {
        qb.push(" AND l.created_at <= ").push_bind(until);
    }
    qb.push(" ORDER BY l.id DESC LIMIT ").push_bind(query.limit);
    qb
}

impl PersistHandle {
    pub async fn list_labels(
        &self,
        query: &LabelQuery,
    ) -> Result<Vec<EvalLabelListRow>, PersistError> {
        crate::metrics::time_db("list_labels", async move {
            let rows = list_labels_query(query)
                .build_query_as::<EvalLabelListRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn eval_vocabulary(&self) -> Result<Vec<VocabularyRow>, PersistError> {
        crate::metrics::time_db("eval_vocabulary", async move {
            let rows = sqlx::query_as!(
                VocabularyRow,
                "SELECT axis, label, subject_kinds, sort_order, description
                   FROM eval.axis_labels ORDER BY axis, sort_order, label",
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn import_batches(&self) -> Result<Vec<ImportBatchRow>, PersistError> {
        crate::metrics::time_db("import_batches", async move {
            let rows = sqlx::query_as!(
                ImportBatchRow,
                "SELECT id, source_key, manifest, counts, imported_at, imported_by
                   FROM eval.import_batches ORDER BY id",
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn find_quote_turn(
        &self,
        session_uuid: uuid::Uuid,
        quote: &str,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("find_quote_turn", async move {
            let id = sqlx::query_scalar!(
                "SELECT t.id FROM memory.operator_turns t
                   JOIN sessions.sessions s ON s.id = t.session_id
                  WHERE s.session_uuid = $1
                    AND (t.opened_by IS NULL OR t.opened_by = 'operator')
                    AND strpos(t.user_prompt, $2) > 0
                  ORDER BY t.created_at DESC, t.id DESC
                  LIMIT 1",
                session_uuid,
                quote,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn recent_relay_callers(&self, quote: &str) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("recent_relay_callers", async move {
            let sessions = sqlx::query_scalar!(
                r#"SELECT DISTINCT s.session_uuid::text AS "session_uuid!"
                     FROM tools.uses u JOIN sessions.sessions s ON s.id = u.session_id
                    WHERE u.created_at > now() - interval '15 minutes'
                      AND u.tool_name LIKE 'mcp\_\_%\_\_relay\_label'
                      AND u.input->>'operator_quote' = $1"#,
                quote,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(sessions)
        })
        .await
    }

    pub async fn verdict_now(
        &self,
        kind: &str,
        turn_id: i64,
    ) -> Result<Option<serde_json::Value>, PersistError> {
        crate::metrics::time_db("verdict_now", async move {
            let value = if kind == "session_end" {
                sqlx::query_scalar!(
                    r#"SELECT jsonb_build_object('outcome', o.code, 'judge_model', v.judge_model,
                              'rubric_version', v.rubric_version, 'bundle_id', v.bundle_id,
                              'judged_at', v.judged_at, 'ending_turn_id', t.id) AS "verdict!"
                         FROM memory.operator_turns t
                         JOIN session_judges.verdicts v ON v.session_id = t.session_id
                         JOIN session_judges.outcomes o ON o.id = v.outcome_id
                        WHERE t.id = $1
                          AND v.mode = 'live'
                        ORDER BY v.judged_at DESC, v.id DESC LIMIT 1"#,
                    turn_id,
                )
                .fetch_optional(&self.pool)
                .await?
            } else {
                sqlx::query_scalar!(
                    r#"SELECT jsonb_build_object('outcome', o.code, 'judge_model', t.judge_model,
                              'rubric_version', t.judge_rubric_version,
                              'bundle_id', t.outcome_bundle_id,
                              'classified_at', t.classified_at) AS "verdict!"
                         FROM memory.operator_turns t
                         JOIN memory.turn_outcomes o ON o.id = t.outcome_id
                        WHERE t.id = $1"#,
                    turn_id,
                )
                .fetch_optional(&self.pool)
                .await?
            };
            Ok(value)
        })
        .await
    }

    pub async fn session_ending_turn(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("session_ending_turn", async move {
            let id = sqlx::query_scalar!(
                "SELECT t.id FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
                  WHERE s.session_uuid = $1 AND t.closed_at IS NOT NULL
                  ORDER BY t.closed_at DESC, t.id DESC LIMIT 1",
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }
}
