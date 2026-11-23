use crate::store::persistence::PersistError;
use crate::store::persistence::recall_embedding::set_vector_scan_limits;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct NewStatement {
    pub text: String,
    pub passage_start: Option<i32>,
    pub passage_end: Option<i32>,
    pub embedding: String,
}

#[derive(Debug, Clone)]
pub struct NewStatements {
    pub split_hash: String,
    pub model_key: String,
    pub stated_at: DateTime<Utc>,
    pub judge_after: DateTime<Utc>,
    pub statements: Vec<NewStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedStatement {
    pub id: i64,
    pub ordinal: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoneStatement {
    pub id: i64,
    pub subject_id: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplacedStatements {
    pub created: Vec<CreatedStatement>,
    pub gone: Vec<GoneStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NearStatement {
    pub statement_id: i64,
    pub subject_id: Option<i64>,
    pub item_id: i64,
    pub similarity: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementStateChange {
    pub statement_id: i64,
    pub state: String,
    pub resolved_by_statement_id: Option<i64>,
}

type Tx<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn replace_item_statements(
        self,
        item_id: i64,
        new: &NewStatements,
    ) -> Result<ReplacedStatements, PersistError> {
        let payload: Value = serde_json::to_value(&new.statements)?;
        let mut tx = self.pool().begin().await?;
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/replace_statements.sql",
            item_id,
            &new.split_hash,
            &new.model_key,
            new.stated_at,
            payload
        )
        .fetch_all(&mut *tx)
        .await?;
        let mut replaced = ReplacedStatements::default();
        for row in rows {
            match (row.outcome.as_str(), row.ordinal) {
                ("created", Some(ordinal)) => replaced.created.push(CreatedStatement {
                    id: row.id,
                    ordinal,
                }),
                _ => replaced.gone.push(GoneStatement {
                    id: row.id,
                    subject_id: row.subject_id,
                }),
            }
        }
        settle_subjects(&mut tx, &replaced.gone, new.judge_after).await?;
        tx.commit().await?;
        Ok(replaced)
    }

    pub async fn mark_item_statements_gone(
        self,
        item_id: i64,
        judge_after: DateTime<Utc>,
    ) -> Result<Vec<GoneStatement>, PersistError> {
        let mut tx = self.pool().begin().await?;
        let gone: Vec<GoneStatement> = sqlx::query!(
            r#"UPDATE memory.knowledge_statements
                  SET state = 'gone', resolved_by_statement_id = NULL
                WHERE item_id = $1 AND state <> 'gone'
               RETURNING id, subject_id"#,
            item_id
        )
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|row| GoneStatement {
            id: row.id,
            subject_id: row.subject_id,
        })
        .collect();
        settle_subjects(&mut tx, &gone, judge_after).await?;
        tx.commit().await?;
        Ok(gone)
    }

    pub async fn set_statement_states(
        self,
        changes: &[StatementStateChange],
    ) -> Result<u64, PersistError> {
        if changes.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = changes.iter().map(|c| c.statement_id).collect();
        let states: Vec<String> = changes.iter().map(|c| c.state.clone()).collect();
        let resolvers: Vec<i64> = changes
            .iter()
            .map(|c| c.resolved_by_statement_id.unwrap_or(0))
            .collect();
        let mut tx = self.pool().begin().await?;
        let subjects = sqlx::query_scalar!(
            r#"UPDATE memory.knowledge_statements s
                  SET state = c.state, resolved_by_statement_id = NULLIF(c.resolver, 0)
                 FROM unnest($1::bigint[], $2::text[], $3::bigint[]) AS c(id, state, resolver)
                WHERE s.id = c.id
               RETURNING s.subject_id"#,
            &ids,
            &states,
            &resolvers
        )
        .fetch_all(&mut *tx)
        .await?;
        let touched = subjects.len() as u64;
        let subject_ids: Vec<i64> = subjects.into_iter().flatten().collect();
        sqlx::query!(
            "SELECT memory.refresh_knowledge_subjects($1::bigint[])",
            &subject_ids
        )
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(touched)
    }

    pub async fn nearest_statements(
        self,
        project_id: Option<i64>,
        embedding: &str,
        exclude_statement_ids: &[i64],
        k: i64,
    ) -> Result<Vec<NearStatement>, PersistError> {
        let mut tx = self.pool().begin().await?;
        set_vector_scan_limits(&mut tx).await?;
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/nearest_statements.sql",
            project_id,
            embedding,
            exclude_statement_ids,
            k
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|row| NearStatement {
                statement_id: row.statement_id,
                subject_id: row.subject_id,
                item_id: row.item_id,
                similarity: row.similarity,
            })
            .collect())
    }
}

async fn settle_subjects(
    tx: &mut Tx<'_>,
    gone: &[GoneStatement],
    judge_after: DateTime<Utc>,
) -> Result<(), PersistError> {
    let subject_ids: Vec<i64> = gone.iter().filter_map(|g| g.subject_id).collect();
    if subject_ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "SELECT memory.refresh_knowledge_subjects($1::bigint[]),
                memory.mark_knowledge_subjects_changed($1::bigint[], $2::timestamptz)",
        &subject_ids,
        judge_after
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}
