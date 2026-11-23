use crate::store::persistence::PersistError;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct StatementDetail {
    pub statement_id: i64,
    pub item_id: i64,
    pub ordinal: i32,
    pub text: String,
    pub passage_start: Option<i32>,
    pub passage_end: Option<i32>,
    pub carrier_reference: String,
    pub kind: String,
    pub source_name: Option<String>,
    pub source_id: i64,
    pub project_id: Option<i64>,
    pub stated_at: DateTime<Utc>,
    pub subject_id: Option<i64>,
    pub state: String,
    pub relative_path: Option<String>,
    pub scope: Option<String>,
    pub head: Option<String>,
    pub resolved_by_statement_id: Option<i64>,
    pub passage: Option<String>,
    pub lesson_active: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemStatement {
    pub statement_id: i64,
    pub text: String,
    pub subject_id: Option<i64>,
    pub split_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLocation {
    pub item_id: i64,
    pub machine_uuid: Option<Uuid>,
    pub root_path: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Backlog {
    pub items: i64,
    pub files: i64,
    pub subjects: i64,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn statement_details(
        self,
        statement_ids: &[i64],
    ) -> Result<Vec<StatementDetail>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/statement_details.sql",
            statement_ids
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| StatementDetail {
                statement_id: row.statement_id,
                item_id: row.item_id,
                ordinal: row.ordinal,
                text: row.text,
                passage_start: row.passage_start,
                passage_end: row.passage_end,
                carrier_reference: row.carrier_reference,
                kind: row.kind,
                source_name: row.source_name,
                source_id: row.source_id,
                project_id: row.project_id,
                stated_at: row.stated_at,
                subject_id: row.subject_id,
                state: row.state,
                relative_path: row.relative_path,
                scope: row.scope,
                head: row.head,
                resolved_by_statement_id: row.resolved_by_statement_id,
                passage: row.passage,
                lesson_active: row.lesson_active,
            })
            .collect())
    }

    pub async fn item_current_statements(
        self,
        item_id: i64,
    ) -> Result<Vec<ItemStatement>, PersistError> {
        Ok(sqlx::query!(
            "SELECT id, text, subject_id, split_hash FROM memory.knowledge_statements
              WHERE item_id = $1 AND state <> 'gone' ORDER BY ordinal",
            item_id
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|row| ItemStatement {
            statement_id: row.id,
            text: row.text,
            subject_id: row.subject_id,
            split_hash: row.split_hash,
        })
        .collect())
    }

    pub async fn item_chunk_embeddings(self, item_id: i64) -> Result<Vec<String>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT c.embedding::text AS "embedding!"
                 FROM memory.knowledge_embedding_chunks c
                 JOIN memory.knowledge_embedding_state e ON e.id = c.state_id
                WHERE e.item_id = $1
                ORDER BY c.ordinal"#,
            item_id
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn consolidation_judgements_since(
        self,
        since: DateTime<Utc>,
    ) -> Result<i64, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM (
                   SELECT DISTINCT run_id, subject_id
                     FROM memory.knowledge_consolidation_decisions
                    WHERE created_at >= $1 AND subject_id IS NOT NULL) x"#,
            since
        )
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn consolidation_backlog(self) -> Result<Backlog, PersistError> {
        let row = sqlx::query!(
            r#"SELECT (SELECT count(*) FROM memory.knowledge_items WHERE consolidate_at IS NOT NULL) AS "items!",
                      (SELECT count(*) FROM memory.knowledge_items
                        WHERE consolidate_at IS NOT NULL AND kind = 'artifact') AS "files!",
                      (SELECT count(*) FROM memory.knowledge_subjects
                        WHERE changed_at IS NOT NULL AND members > 0) AS "subjects!""#
        )
        .fetch_one(self.pool())
        .await?;
        Ok(Backlog {
            items: row.items,
            files: row.files,
            subjects: row.subjects,
        })
    }

    pub async fn knowledge_file_locations(
        self,
        item_ids: &[i64],
    ) -> Result<Vec<FileLocation>, PersistError> {
        Ok(sqlx::query!(
            r#"SELECT i.id AS "item_id!", COALESCE(s.machine_uuid, loc.machine_uuid) AS machine_uuid,
                      s.root_path AS "root_path!", d.relative_path AS "relative_path!"
                 FROM memory.knowledge_items i
                 JOIN memory.knowledge_revisions r ON r.id = i.source_id
                 JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid
                 JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
                 LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
                WHERE i.id = ANY($1) AND i.kind = 'artifact'
                ORDER BY i.id"#,
            item_ids
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|row| FileLocation {
            item_id: row.item_id,
            machine_uuid: row.machine_uuid,
            root_path: row.root_path,
            relative_path: row.relative_path,
        })
        .collect())
    }

    pub async fn lesson_id_of_item(self, item_id: i64) -> Result<Option<i64>, PersistError> {
        Ok(sqlx::query_scalar!(
            "SELECT source_id FROM memory.knowledge_items WHERE id = $1 AND kind = 'lesson'",
            item_id
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn knowledge_subject_centre(
        self,
        subject_id: i64,
    ) -> Result<Option<String>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT centre::text AS "centre!" FROM memory.knowledge_subjects WHERE id = $1"#,
            subject_id
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn statement_embeddings(
        self,
        statement_ids: &[i64],
    ) -> Result<Vec<(i64, String)>, PersistError> {
        Ok(sqlx::query!(
            r#"SELECT id AS "id!", embedding::text AS "embedding!"
                 FROM memory.knowledge_statements WHERE id = ANY($1) AND item_id IS NOT NULL"#,
            statement_ids
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|row| (row.id, row.embedding))
        .collect())
    }

    pub async fn consolidation_due(self) -> Result<Backlog, PersistError> {
        let row = sqlx::query!(
            r#"SELECT (SELECT count(*) FROM memory.knowledge_items
                        WHERE consolidate_at IS NOT NULL AND consolidate_at <= now() AND NOT needs_embedding) AS "items!",
                      (SELECT count(*) FROM memory.knowledge_items
                        WHERE consolidate_at IS NOT NULL AND consolidate_at <= now() AND NOT needs_embedding
                          AND kind = 'artifact') AS "files!",
                      (SELECT count(*) FROM memory.knowledge_subjects
                        WHERE changed_at IS NOT NULL AND judge_after <= now() AND members > 0) AS "subjects!""#
        )
        .fetch_one(self.pool())
        .await?;
        Ok(Backlog {
            items: row.items,
            files: row.files,
            subjects: row.subjects,
        })
    }

    pub async fn subject_statement_ids_all(self, subject_id: i64) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_scalar!(
            "SELECT id FROM memory.knowledge_statements WHERE subject_id = $1 AND item_id IS NOT NULL ORDER BY stated_at, id",
            subject_id
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn proposed_decisions_on_machine(
        self,
        machine: Uuid,
        limit: i64,
    ) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_file_scalar!(
            "src/store/persistence/consolidation/proposed_on_machine.sql",
            machine,
            limit
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn close_stale_decision(self, decision_id: i64, note: &str) -> Result<bool, PersistError> {
        Ok(sqlx::query!(
            "UPDATE memory.knowledge_consolidation_decisions
                SET application = 'none', error = NULL, reason = reason || ' (' || $2 || ')'
              WHERE id = $1 AND application IN ('proposed', 'failed')",
            decision_id,
            note
        )
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn stamp_item_split_hash(self, item_id: i64, split_hash: &str) -> Result<u64, PersistError> {
        Ok(sqlx::query!(
            "UPDATE memory.knowledge_statements SET split_hash = $2 WHERE item_id = $1 AND state <> 'gone'",
            item_id,
            split_hash
        )
        .execute(self.pool())
        .await?
        .rows_affected())
    }
}
