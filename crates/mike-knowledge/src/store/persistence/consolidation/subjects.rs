use crate::store::persistence::PersistError;
use crate::store::persistence::recall_embedding::set_vector_scan_limits;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct DueSubject {
    pub id: i64,
    pub project_id: Option<i64>,
    pub changed_at: DateTime<Utc>,
    pub members: i32,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectStatement {
    pub statement_id: i64,
    pub item_id: i64,
    pub ordinal: i32,
    pub text: String,
    pub passage_start: Option<i32>,
    pub passage_end: Option<i32>,
    pub carrier_reference: String,
    pub kind: String,
    pub source_name: Option<String>,
    pub stated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NearSubject {
    pub subject_id: i64,
    pub similarity: f64,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn create_knowledge_subject(
        self,
        project_id: Option<i64>,
        first_statement_id: i64,
        judge_after: DateTime<Utc>,
    ) -> Result<i64, PersistError> {
        let mut tx = self.pool().begin().await?;
        let subject_id = sqlx::query_scalar!(
            "INSERT INTO memory.knowledge_subjects (project_id, centre)
             SELECT $1, s.embedding FROM memory.knowledge_statements s WHERE s.id = $2
             RETURNING id",
            project_id,
            first_statement_id
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            "UPDATE memory.knowledge_statements SET subject_id = $1 WHERE id = $2",
            subject_id,
            first_statement_id
        )
        .execute(&mut *tx)
        .await?;
        settle(&mut tx, &[subject_id], judge_after).await?;
        tx.commit().await?;
        Ok(subject_id)
    }

    pub async fn add_statement_to_subject(
        self,
        statement_id: i64,
        subject_id: i64,
        judge_after: DateTime<Utc>,
    ) -> Result<(), PersistError> {
        self.move_statement(statement_id, Some(subject_id), judge_after)
            .await
    }

    pub async fn remove_statement_from_subject(
        self,
        statement_id: i64,
        judge_after: DateTime<Utc>,
    ) -> Result<(), PersistError> {
        self.move_statement(statement_id, None, judge_after).await
    }

    async fn move_statement(
        self,
        statement_id: i64,
        subject_id: Option<i64>,
        judge_after: DateTime<Utc>,
    ) -> Result<(), PersistError> {
        let mut tx = self.pool().begin().await?;
        let previous = sqlx::query_scalar!(
            "SELECT subject_id FROM memory.knowledge_statements WHERE id = $1 FOR UPDATE",
            statement_id
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            "UPDATE memory.knowledge_statements SET subject_id = $2 WHERE id = $1",
            statement_id,
            subject_id
        )
        .execute(&mut *tx)
        .await?;
        let touched: Vec<i64> = previous.into_iter().chain(subject_id).collect();
        settle(&mut tx, &touched, judge_after).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn mark_knowledge_subjects_changed(
        self,
        subject_ids: &[i64],
        judge_after: DateTime<Utc>,
    ) -> Result<i64, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT memory.mark_knowledge_subjects_changed($1::bigint[], $2::timestamptz) AS "marked!""#,
            subject_ids,
            judge_after
        )
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn claim_due_knowledge_subjects(
        self,
        limit: i64,
    ) -> Result<Vec<DueSubject>, PersistError> {
        Ok(sqlx::query!(
            r#"SELECT id, project_id, changed_at AS "changed_at!", members, title, text
                 FROM memory.knowledge_subjects
                WHERE changed_at IS NOT NULL AND judge_after <= now() AND members > 0
                ORDER BY judge_after, id
                LIMIT $1
                  FOR UPDATE SKIP LOCKED"#,
            limit
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|row| DueSubject {
            id: row.id,
            project_id: row.project_id,
            changed_at: row.changed_at,
            members: row.members,
            title: row.title,
            text: row.text,
        })
        .collect())
    }

    pub async fn finish_knowledge_subject_judgement(
        self,
        subject_id: i64,
        claimed_changed_at: DateTime<Utc>,
        concise: Option<(&str, &str)>,
    ) -> Result<bool, PersistError> {
        let (title, text) = concise.unzip();
        Ok(sqlx::query!(
            "UPDATE memory.knowledge_subjects
                SET judged_at = now(), changed_at = NULL, judge_after = NULL,
                    title = COALESCE($3, title), text = COALESCE($4, text)
              WHERE id = $1 AND changed_at = $2",
            subject_id,
            claimed_changed_at,
            title,
            text
        )
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn drop_empty_knowledge_subjects(self) -> Result<u64, PersistError> {
        Ok(
            sqlx::query!("DELETE FROM memory.knowledge_subjects WHERE members = 0")
                .execute(self.pool())
                .await?
                .rows_affected(),
        )
    }

    pub async fn knowledge_subject_statements(
        self,
        subject_id: i64,
    ) -> Result<Vec<SubjectStatement>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/subject_statements.sql",
            subject_id
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| SubjectStatement {
                statement_id: row.statement_id,
                item_id: row.item_id,
                ordinal: row.ordinal,
                text: row.text,
                passage_start: row.passage_start,
                passage_end: row.passage_end,
                carrier_reference: row.carrier_reference,
                kind: row.kind,
                source_name: row.source_name,
                stated_at: row.stated_at,
            })
            .collect())
    }

    pub async fn nearest_knowledge_subjects(
        self,
        subject_id: i64,
        k: i64,
    ) -> Result<Vec<NearSubject>, PersistError> {
        let mut tx = self.pool().begin().await?;
        set_vector_scan_limits(&mut tx).await?;
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/nearest_subjects.sql",
            subject_id,
            k
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|row| NearSubject {
                subject_id: row.subject_id,
                similarity: row.similarity,
            })
            .collect())
    }
}

async fn settle(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    subject_ids: &[i64],
    judge_after: DateTime<Utc>,
) -> Result<(), PersistError> {
    sqlx::query!(
        "SELECT memory.refresh_knowledge_subjects($1::bigint[]),
                memory.mark_knowledge_subjects_changed($1::bigint[], $2::timestamptz)",
        subject_ids,
        judge_after
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}
