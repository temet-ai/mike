use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::invalid;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Deserialize)]
pub struct LessonImport {
    pub location_uuid: Uuid,
    pub expected_version: i64,
    pub payload: Value,
}

impl PersistHandle {
    pub async fn lesson_files(&self, location: Uuid) -> Result<Value, PersistError> {
        let project = self.project_for_location(location).await?;
        let rows = sqlx::query_scalar!(
            r#"SELECT jsonb_build_object('version', c.version, 'payload', c.payload,
                 'written_version', r.version) AS "lesson!"
             FROM memory.lesson_file_changes c
             LEFT JOIN memory.lesson_file_receipts r ON r.lesson_uuid = c.lesson_uuid
               AND r.location_uuid = $2
             WHERE c.project_id = $1 ORDER BY c.lesson_uuid"#,
            project.id,
            location,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(json!({"project_uuid": project.project_uuid, "lessons": rows}))
    }

    pub(crate) async fn acknowledge_lesson_files(
        &self,
        location: Uuid,
        receipts: Value,
    ) -> Result<(), PersistError> {
        let mut tx = self.pool().begin().await?;
        sqlx::query!("INSERT INTO memory.lesson_file_receipts(location_uuid, lesson_uuid, version)
             SELECT $1, c.lesson_uuid, c.version
             FROM jsonb_to_recordset($2) AS x(lesson_uuid uuid, version bigint)
             JOIN memory.lesson_file_changes c ON c.lesson_uuid = x.lesson_uuid AND c.version = x.version
             JOIN projects.locations l ON l.location_uuid = $1 AND l.project_id = c.project_id
             ON CONFLICT(location_uuid, lesson_uuid) DO UPDATE
               SET version = EXCLUDED.version, written_at = now()", location, &receipts).execute(&mut *tx).await?;
        sqlx::query!(
            "DELETE FROM memory.lesson_file_conflicts k
             USING memory.lesson_file_receipts r,
                   jsonb_to_recordset($2) AS x(lesson_uuid uuid)
             WHERE k.location_uuid = $1 AND k.lesson_uuid = x.lesson_uuid
               AND r.location_uuid = k.location_uuid AND r.lesson_uuid = k.lesson_uuid
               AND r.version >= k.server_version",
            location,
            &receipts
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn import_lesson_file(&self, input: &LessonImport) -> Result<Value, PersistError> {
        let project = self.project_for_location(input.location_uuid).await?;
        let payload = &input.payload;
        let (uuid, body) = lesson_identity(payload, &project.project_uuid.to_string())?;
        let hash: Vec<u8> = Sha256::digest(body.trim().to_lowercase().as_bytes()).to_vec();
        let mut tx = self.pool().begin().await?;
        let current_lesson = lock_current_lesson(&mut tx, uuid).await?;
        ensure_importable(current_lesson, input.expected_version, project.id, payload)?;
        ensure_agent_scope(&mut tx, payload).await?;
        let inserted = sqlx::query_file_scalar!(
            "src/store/persistence/project_knowledge/lesson_import.sql",
            project.id,
            uuid,
            payload,
            &hash,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let id = inserted
            .ok_or_else(|| invalid("Unknown lesson kind, lifecycle, source or agent scope"))?;
        restore_relations(&mut tx, id, project.id, payload).await?;
        let written = sqlx::query!(
            "SELECT version, payload FROM memory.lesson_file_changes WHERE lesson_uuid = $1",
            uuid,
        )
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(json!({"lesson_uuid": uuid, "version": written.version, "payload": written.payload}))
    }
}

#[derive(Clone, Copy)]
struct CurrentLesson {
    version: i64,
    project_id: i64,
    deleted: bool,
}

async fn lock_current_lesson(
    tx: &mut sqlx::PgConnection,
    uuid: Uuid,
) -> Result<Option<CurrentLesson>, PersistError> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))::text AS locked",
        uuid.to_string()
    )
    .fetch_one(&mut *tx)
    .await?;
    let current = sqlx::query!(
        "SELECT version, project_id, payload FROM memory.lesson_file_changes WHERE lesson_uuid = $1 FOR UPDATE",
        uuid,
    )
    .fetch_optional(&mut *tx)
    .await?;
    Ok(current.as_ref().map(|v| CurrentLesson {
        version: v.version,
        project_id: v.project_id,
        deleted: v.payload["deleted"] == true,
    }))
}

fn lesson_identity<'a>(
    payload: &'a Value,
    project_uuid: &str,
) -> Result<(Uuid, &'a str), PersistError> {
    let uuid: Uuid = payload
        .get("lesson_uuid")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid("Invalid lesson UUID"))?;
    if payload.get("project_uuid").and_then(Value::as_str) != Some(project_uuid) {
        return Err(invalid("Lesson file belongs to another project"));
    }
    let body = payload
        .get("body")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid("Lesson body is empty"))?;
    Ok((uuid, body))
}

fn ensure_importable(
    current: Option<CurrentLesson>,
    expected_version: i64,
    project_id: i64,
    payload: &Value,
) -> Result<(), PersistError> {
    if current.map(|c| c.version).unwrap_or(0) != expected_version
        || current.is_some_and(|c| c.project_id != project_id)
    {
        return Err(invalid(
            "Lesson sync conflict; neither file nor database was overwritten",
        ));
    }
    if current.is_some_and(|c| c.deleted)
        || payload.get("deleted").and_then(Value::as_bool) == Some(true)
    {
        return Err(invalid(
            "Deleted lesson files cannot be imported as active lessons",
        ));
    }
    Ok(())
}

async fn restore_relations(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    project: i64,
    payload: &Value,
) -> Result<(), PersistError> {
    let relations = payload
        .get("relations")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let expected = relations
        .as_array()
        .ok_or_else(|| invalid("relations must be an array"))?
        .len();
    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM jsonb_to_recordset($1) AS x(target_uuid uuid)
         JOIN memory.lessons l ON l.uuid = x.target_uuid AND l.project_id = $2"#,
        &relations,
        project,
    )
    .fetch_one(&mut **tx)
    .await?;
    if count != expected as i64 {
        return Err(invalid(
            "Related lessons are missing; import their files first",
        ));
    }
    sqlx::query!(
        "DELETE FROM memory.lesson_relations r
        USING memory.lessons target, memory.lesson_relation_kinds k
        WHERE r.from_lesson_id = $1 AND target.id = r.to_lesson_id AND k.id = r.kind_id
          AND NOT EXISTS (SELECT 1 FROM jsonb_to_recordset($2) AS x(target_uuid uuid, kind text)
              WHERE x.target_uuid = target.uuid AND x.kind = k.code)",
        id,
        &relations
    )
    .execute(&mut **tx)
    .await?;
    let changed = sqlx::query_file!(
        "src/store/persistence/project_knowledge/lesson_relations_import.sql",
        id,
        relations,
        project
    )
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if changed != expected as u64 {
        return Err(invalid("Invalid relationship metadata"));
    }
    Ok(())
}

async fn ensure_agent_scope(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    payload: &Value,
) -> Result<(), PersistError> {
    let Some(uuid) = payload.get("agent_uuid").and_then(Value::as_str) else {
        return Ok(());
    };
    let uuid: Uuid = uuid.parse().map_err(|_| invalid("Invalid agent UUID"))?;
    let name = payload
        .get("agent_name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid("Agent name missing"))?;
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))::text AS locked",
        format!("agent:{uuid}")
    )
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query!(
        "INSERT INTO agents.agents(uuid, name) SELECT $1, $2
        WHERE NOT EXISTS (SELECT 1 FROM agents.agents WHERE uuid = $1)",
        uuid,
        name
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
