use sha2::{Digest as _, Sha256};

use sqlx::PgConnection;

use super::shape::insert_shape_finding;
use super::{LessonInsertRow, NewLessonWrite};
use crate::lesson_shape::Assessment;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(sqlx::FromRow)]
struct InsertedRow {
    id: i64,
    uuid: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<InsertedRow> for LessonInsertRow {
    fn from(row: InsertedRow) -> Self {
        LessonInsertRow {
            id: row.id,
            uuid: row.uuid,
            created_at: row.created_at,
            expires_at: row.expires_at,
        }
    }
}

async fn insert_operator_lesson(
    conn: &mut PgConnection,
    new: &NewLessonWrite<'_>,
) -> Result<LessonInsertRow, PersistError> {
    let row = fetch_inserted(conn, new).await?;
    log_lesson_created(&row);
    Ok(row.into())
}

fn log_lesson_created(row: &InsertedRow) {
    tracing::debug!(lesson.id = row.id, lesson.uuid = %row.uuid, "lesson created");
}

async fn fetch_inserted(
    conn: &mut PgConnection,
    new: &NewLessonWrite<'_>,
) -> Result<InsertedRow, sqlx::Error> {
    let normalised = new.text.trim().to_lowercase();
    let text_sha256: Vec<u8> = Sha256::digest(normalised.as_bytes()).to_vec();
    sqlx::query_as!(
        InsertedRow,
        "INSERT INTO memory.lessons
                    (project_id, kind_id, text, text_sha256, topic, stage_id,
                     source_turn_id, source_id, agent_id, session_id, request_id)
                 SELECT $1, lk.id, $3, $4, $5, ls.id, $6, lsrc.id, $7, $8, $9
                 FROM memory.lesson_kinds lk,
                      memory.lesson_stages ls,
                      memory.lesson_sources lsrc
                 WHERE lk.code = $2
                   AND ls.code = 'raw'
                   AND lsrc.code = 'operator'
                 RETURNING id, uuid, created_at, expires_at",
        new.project_id,
        new.kind,
        new.text,
        &text_sha256,
        new.topic,
        new.source_turn_id,
        new.agent_id,
        new.session_db_id,
        new.request_id,
    )
    .fetch_one(&mut *conn)
    .await
}

impl PersistHandle {
    pub async fn create_lesson_with_topic(
        &self,
        new: NewLessonWrite<'_>,
    ) -> Result<LessonInsertRow, PersistError> {
        crate::metrics::time_db("create_lesson", async move {
            let mut conn = self.pool.acquire().await?;
            insert_operator_lesson(&mut conn, &new).await
        })
        .await
    }

    pub async fn create_lesson_with_shape(
        &self,
        new: NewLessonWrite<'_>,
        assessment: &Assessment,
        hold: bool,
    ) -> Result<LessonInsertRow, PersistError> {
        crate::metrics::time_db("create_lesson_with_shape", async move {
            let mut tx = self.pool.begin().await?;
            let row = insert_operator_lesson(&mut tx, &new).await?;
            insert_shape_finding(&mut tx, row.id, assessment, hold).await?;
            tx.commit().await?;
            Ok(row)
        })
        .await
    }

    pub async fn create_harvested_lesson(
        &self,
        new: NewLessonWrite<'_>,
    ) -> Result<Option<LessonInsertRow>, PersistError> {
        crate::metrics::time_db("create_harvested_lesson", async move {
            let normalised = new.text.trim().to_lowercase();
            let text_sha256: Vec<u8> = Sha256::digest(normalised.as_bytes()).to_vec();

            #[derive(sqlx::FromRow)]
            struct InsertedRow {
                id: i64,
                uuid: uuid::Uuid,
                created_at: chrono::DateTime<chrono::Utc>,
                expires_at: Option<chrono::DateTime<chrono::Utc>>,
            }

            let row: Option<InsertedRow> = sqlx::query_as!(
                InsertedRow,
                "INSERT INTO memory.lessons
                    (project_id, kind_id, text, text_sha256, topic, stage_id,
                     source_turn_id, source_id, agent_id, session_id, request_id)
                 SELECT $1, lk.id, $3, $4, $5, ls.id, $6, lsrc.id, $7, $8, $9
                 FROM memory.lesson_kinds lk,
                      memory.lesson_stages ls,
                      memory.lesson_sources lsrc
                 WHERE lk.code = $2
                   AND ls.code = 'raw'
                   AND lsrc.code = 'harvester'
                   AND NOT EXISTS (
                       SELECT 1 FROM memory.lessons dup
                        WHERE dup.project_id = $1
                          AND dup.text_sha256 = $4
                          AND dup.archived_at IS NULL
                   )
                 RETURNING id, uuid, created_at, expires_at",
                new.project_id,
                new.kind,
                new.text,
                &text_sha256,
                new.topic,
                new.source_turn_id,
                new.agent_id,
                new.session_db_id,
                new.request_id,
            )
            .fetch_optional(&self.pool)
            .await?;

            Ok(row.map(|r| {
                tracing::debug!(
                    lesson.id = r.id,
                    lesson.uuid = %r.uuid,
                    "harvester lesson created"
                );
                LessonInsertRow {
                    id: r.id,
                    uuid: r.uuid,
                    created_at: r.created_at,
                    expires_at: r.expires_at,
                }
            }))
        })
        .await
    }

    pub async fn delete_lesson(&self, id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("delete_lesson", async move {
            let result = sqlx::query!(
                "UPDATE memory.lessons l
                    SET stage_id           = archived_s.id,
                        archived_at        = NOW(),
                        archived_reason_id = operator_r.id
                   FROM memory.lesson_stages archived_s,
                        memory.lesson_archived_reasons operator_r
                  WHERE archived_s.code = 'archived'
                    AND operator_r.code = 'operator'
                    AND l.id = $1
                    AND l.archived_at IS NULL",
                id
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    #[allow(dead_code)]
    pub async fn archive_lesson(
        &self,
        id: i64,
        reason: &str,
        duplicate_of_id: Option<i64>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("archive_lesson", async move {
            sqlx::query!(
                "UPDATE memory.lessons l
                    SET stage_id           = archived_s.id,
                        archived_at        = NOW(),
                        archived_reason_id = ar.id,
                        duplicate_of_id    = $3
                   FROM memory.lesson_stages archived_s,
                        memory.lesson_archived_reasons ar
                  WHERE archived_s.code = 'archived'
                    AND ar.code = $2
                    AND l.id = $1
                    AND l.stage_id != archived_s.id",
                id,
                reason,
                duplicate_of_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}
