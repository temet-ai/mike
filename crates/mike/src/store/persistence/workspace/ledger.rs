use super::CoordinationEventRow;
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) fn actor_label(session_uuid: uuid::Uuid, title: Option<&str>) -> String {
    match title {
        Some(t) if !t.trim().is_empty() => format!("{session_uuid} ({t})"),
        _ => session_uuid.to_string(),
    }
}

pub(super) async fn resolve_actor_label<'e, E>(
    executor: E,
    session_uuid: uuid::Uuid,
    session_id: i64,
) -> Result<String, PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let title: Option<String> =
        sqlx::query_scalar("SELECT title FROM sessions.sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(executor)
            .await?;
    Ok(actor_label(session_uuid, title.as_deref()))
}

pub(super) async fn insert_ledger_event<'e, E>(
    executor: E,
    project_id: i64,
    session_id: Option<i64>,
    actor_label: &str,
    kind_code: &str,
    subject: &str,
) -> Result<(), PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query!(
        "INSERT INTO workspace.coordination_events
             (project_id, session_id, actor_label, kind_id, subject, detail)
         SELECT $1, $2, $3, ek.id, $4, '{}'::jsonb
           FROM workspace.event_kinds ek
          WHERE ek.code = $5",
        project_id,
        session_id,
        actor_label,
        subject,
        kind_code,
    )
    .execute(executor)
    .await?;
    Ok(())
}

impl PersistHandle {
    pub(crate) async fn fetch_coordination_events(
        &self,
        project_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<(i64, Vec<CoordinationEventRow>), PersistError> {
        crate::metrics::time_db("workspace_fetch_coordination_events", async move {
            let total_count = sqlx::query_scalar!(
                r#"SELECT COUNT(*) AS "count!"
                     FROM workspace.coordination_events
                    WHERE project_id = $1"#,
                project_id,
            )
            .fetch_one(&self.pool)
            .await?;

            let rows = sqlx::query!(
                r#"SELECT
                       ce.id          AS "id!",
                       s.session_uuid AS "session_id?",
                       ce.actor_label AS "actor_label!",
                       ek.code        AS "kind!",
                       ce.subject     AS "subject!",
                       ce.detail      AS "detail!",
                       ce.created_at  AS "created_at!"
                   FROM workspace.coordination_events ce
                   JOIN workspace.event_kinds ek ON ek.id = ce.kind_id
                   LEFT JOIN sessions.sessions s ON s.id = ce.session_id
                  WHERE ce.project_id = $1
                  ORDER BY ce.id DESC
                  OFFSET $2 LIMIT $3"#,
                project_id,
                offset,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok((
                total_count,
                rows.into_iter()
                    .map(|r| CoordinationEventRow {
                        id: r.id,
                        session_id: r.session_id,
                        actor_label: r.actor_label,
                        kind: r.kind,
                        subject: r.subject,
                        detail: r.detail,
                        created_at: r.created_at,
                    })
                    .collect(),
            ))
        })
        .await
    }
}

#[cfg(test)]
mod tests;
