use crate::store::persistence::PersistError;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct ClaimedItem {
    pub item_id: i64,
    pub kind: String,
    pub source_name: Option<String>,
    pub project_id: Option<i64>,
    pub reference: String,
    pub body: Option<String>,
    pub source_hash: String,
    pub canonical_item_id: i64,
    pub consolidate_at: DateTime<Utc>,
    pub stated_at: DateTime<Utc>,
    pub active: bool,
    pub detached_statements: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwinCarrier {
    pub item_id: i64,
    pub twin_item_id: i64,
    pub reference: String,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn claim_due_knowledge_items(
        self,
        limit: i64,
    ) -> Result<Vec<ClaimedItem>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/claim_due_items.sql",
            limit
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ClaimedItem {
                item_id: row.item_id,
                kind: row.kind,
                source_name: row.source_name,
                project_id: row.project_id,
                reference: row.reference,
                body: row.body,
                source_hash: row.source_hash,
                canonical_item_id: row.canonical_item_id,
                consolidate_at: row.consolidate_at,
                stated_at: row.stated_at,
                active: row.active,
                detached_statements: row.detached_statements,
            })
            .collect())
    }

    pub async fn mark_knowledge_item_consolidated(
        self,
        item_id: i64,
        claimed_at: DateTime<Utc>,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query!(
            "UPDATE memory.knowledge_items SET consolidate_at = NULL
              WHERE id = $1 AND consolidate_at <= $2",
            item_id,
            claimed_at
        )
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn defer_knowledge_item_consolidation(
        self,
        item_id: i64,
        retry_at: DateTime<Utc>,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query!(
            "UPDATE memory.knowledge_items SET consolidate_at = $2
              WHERE id = $1 AND consolidate_at IS NOT NULL",
            item_id,
            retry_at
        )
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn queue_knowledge_consolidation(
        self,
        item_ids: &[i64],
    ) -> Result<i64, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT memory.queue_knowledge_consolidation($1::bigint[]) AS "queued!""#,
            item_ids
        )
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn twin_carriers(self, item_ids: &[i64]) -> Result<Vec<TwinCarrier>, PersistError> {
        Ok(sqlx::query_as!(
            TwinCarrier,
            r#"SELECT i.id AS "item_id!", tw.twin_item_id AS "twin_item_id!", e.reference AS "reference!"
                 FROM memory.knowledge_items i
                 JOIN memory.knowledge_artifact_twins(ARRAY(
                          SELECT x.source_id FROM memory.knowledge_items x
                           WHERE x.id = ANY($1) AND x.kind = 'artifact')) tw
                   ON tw.revision_id = i.source_id
                 JOIN memory.knowledge_embedding_state e ON e.item_id = tw.twin_item_id AND e.reference IS NOT NULL
                WHERE i.id = ANY($1) AND i.kind = 'artifact'
                ORDER BY i.id, tw.twin_item_id"#,
            item_ids
        )
        .fetch_all(self.pool())
        .await?)
    }
}
