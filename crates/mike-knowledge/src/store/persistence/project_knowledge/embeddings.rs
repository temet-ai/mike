use super::ModelKeys;
use crate::store::persistence::PersistError;
use serde_json::{Value, json};

pub const PENDING_BATCH: i64 = 16;
pub const KNOWLEDGE_COUNT_CAP: i64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnowledgeStateCounts {
    pub ready: i64,
    pub failed: i64,
    pub current_version: i64,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn knowledge_embedding_url_candidates(self) -> Result<Vec<String>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT url AS "url!" FROM (
               SELECT unnest(urls_seen) AS url FROM memory.semantic_embedders
               UNION
               SELECT value #>> '{}' FROM config.settings WHERE key = 'embedding.url'
             ) candidates WHERE url IS NOT NULL"#
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn rekey_knowledge_embeddings(
        self,
        legacy_keys: &[String],
        key: &str,
    ) -> Result<u64, PersistError> {
        let rekeyed = sqlx::query!(
            "WITH items AS (
                 UPDATE memory.knowledge_items SET model_key = $2
                  WHERE model_key = ANY($1) AND model_key <> $2)
             UPDATE memory.knowledge_embedding_state SET model_key = $2
             WHERE model_key = ANY($1) AND model_key <> $2",
            legacy_keys,
            key
        )
        .execute(self.pool())
        .await?
        .rows_affected();
        self.forget_knowledge_coverage();
        Ok(rekeyed)
    }

    pub async fn knowledge_embedding_state_counts(
        self,
        keys: &ModelKeys,
    ) -> Result<KnowledgeStateCounts, PersistError> {
        let counts = sqlx::query!(
            r#"SELECT
                 (SELECT count(*) FROM (
                     SELECT 1 FROM memory.knowledge_embedding_state e
                      WHERE e.model_key = ANY($1::text[]) AND e.completed_at IS NOT NULL
                      LIMIT $2) ready) AS "ready!",
                 (SELECT count(*) FROM (
                     SELECT 1 FROM memory.knowledge_embedding_state e
                      WHERE e.model_key = $3 AND e.completed_at IS NOT NULL
                      LIMIT $2) current_version) AS "current_version!",
                 (SELECT count(*) FROM (
                     SELECT 1 FROM memory.knowledge_items i
                      WHERE i.last_error IS NOT NULL
                      LIMIT $2) failed) AS "failed!""#,
            &keys.accepted(),
            KNOWLEDGE_COUNT_CAP + 1,
            keys.current()
        )
        .fetch_one(self.pool())
        .await?;
        Ok(KnowledgeStateCounts {
            ready: counts.ready,
            failed: counts.failed,
            current_version: counts.current_version,
        })
    }

    pub async fn knowledge_embedding_health(
        self,
        project: Option<i64>,
        keys: &ModelKeys,
    ) -> Result<Value, PersistError> {
        crate::metrics::time_db(
            "knowledge_embedding_health",
            self.count_embedding_health(project, keys),
        )
        .await
    }

    async fn count_embedding_health(
        self,
        project: Option<i64>,
        keys: &ModelKeys,
    ) -> Result<Value, PersistError> {
        let accepted = keys.accepted();
        let (total, ready, failed, current) = match project {
            Some(project) => {
                let counts = sqlx::query_file!(
                    "src/store/persistence/project_knowledge/embedding_health_project.sql",
                    project,
                    &accepted,
                    keys.current()
                )
                .fetch_one(self.pool())
                .await?;
                (
                    counts.total,
                    counts.ready,
                    counts.failed,
                    counts.current_version,
                )
            }
            None => {
                let counts = sqlx::query_file!(
                    "src/store/persistence/project_knowledge/embedding_health_all.sql",
                    &accepted,
                    keys.current()
                )
                .fetch_one(self.pool())
                .await?;
                (
                    counts.total,
                    counts.ready,
                    counts.failed,
                    counts.current_version,
                )
            }
        };
        Ok(
            json!({"total":total,"ready":ready,"pending":total-ready,"failed":failed,
            "current_version":current}),
        )
    }
}
