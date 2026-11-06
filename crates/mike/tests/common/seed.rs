#![allow(dead_code)]

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub async fn database_now(pool: &PgPool) -> anyhow::Result<DateTime<Utc>> {
    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await?)
}

pub async fn seed_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let now = Utc::now();
    let id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, $2, $2
             WHERE NOT EXISTS (SELECT 1 FROM projects.projects WHERE project_path = $1)
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn seed_session(pool: &PgPool, project_id: i64, session_id: &str) -> anyhow::Result<()> {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

enum SessionRef<'a> {
    Pk(i64),
    Uuid { uuid: &'a str, ensure: bool },
}

pub struct SeedRequest<'a> {
    id: i64,
    project_id: i64,
    session: Option<SessionRef<'a>>,
    turn_origin: &'a str,
    status: &'a str,
    turn_type: &'a str,
    subagent_persona: Option<&'a str>,
    created_at: DateTime<Utc>,
}

impl<'a> SeedRequest<'a> {
    pub fn new(id: i64, project_id: i64) -> Self {
        Self {
            id,
            project_id,
            session: None,
            turn_origin: "primary",
            status: "complete",
            turn_type: "user_prompt",
            subagent_persona: None,
            created_at: Utc::now(),
        }
    }

    pub fn session_pk(mut self, pk: i64) -> Self {
        self.session = Some(SessionRef::Pk(pk));
        self
    }

    pub fn session_uuid(mut self, uuid: &'a str) -> Self {
        self.session = Some(SessionRef::Uuid {
            uuid,
            ensure: false,
        });
        self
    }

    pub fn session_uuid_ensure(mut self, uuid: &'a str) -> Self {
        self.session = Some(SessionRef::Uuid { uuid, ensure: true });
        self
    }

    pub fn turn_origin(mut self, origin: &'a str) -> Self {
        self.turn_origin = origin;
        self
    }

    pub fn status(mut self, status: &'a str) -> Self {
        self.status = status;
        self
    }

    pub fn turn_type(mut self, turn_type: &'a str) -> Self {
        self.turn_type = turn_type;
        self
    }

    pub fn subagent_persona(mut self, persona: Option<&'a str>) -> Self {
        self.subagent_persona = persona;
        self
    }

    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }

    pub async fn insert(self, pool: &PgPool) -> anyhow::Result<i64> {
        let session = self
            .session
            .ok_or_else(|| anyhow::anyhow!("SeedRequest: session ref not set"))?;

        match session {
            SessionRef::Pk(pk) => {
                sqlx::query(
                    "INSERT INTO requests.requests (
                        id, session_id, project_id, method_id, path, is_streaming,
                        message_count, is_tool_loop, status_id, params,
                        last_user_message, human_prompt, turn_type_id,
                        turn_origin_id, subagent_persona, created_at
                     )
                     OVERRIDING SYSTEM VALUE
                     SELECT $1, $2, $3, rm.id, '/v1/messages', false,
                            0, false, rs.id, '{}'::jsonb,
                            '', '', rtt.id,
                            rto.id, $4, $5
                     FROM requests.methods rm, requests.statuses rs,
                          requests.turn_types rtt, requests.turn_origins rto
                     WHERE rm.code = 'POST' AND rs.code = $6
                       AND rtt.code = $7 AND rto.code = $8",
                )
                .bind(self.id)
                .bind(pk)
                .bind(self.project_id)
                .bind(self.subagent_persona)
                .bind(self.created_at)
                .bind(self.status)
                .bind(self.turn_type)
                .bind(self.turn_origin)
                .execute(pool)
                .await?;
            }
            SessionRef::Uuid { uuid, ensure } => {
                if ensure {
                    sqlx::query(
                        "INSERT INTO sessions.sessions
                            (session_uuid, project_id, first_seen, last_seen)
                         VALUES ($1::text::uuid, $2, $3, $3)
                         ON CONFLICT (session_uuid) DO NOTHING",
                    )
                    .bind(uuid)
                    .bind(self.project_id)
                    .bind(self.created_at)
                    .execute(pool)
                    .await?;
                }
                let session_uuid: uuid::Uuid = uuid.parse()?;
                sqlx::query(
                    "INSERT INTO requests.requests (
                        id, session_id, project_id, method_id, path, is_streaming,
                        message_count, is_tool_loop, status_id, params,
                        last_user_message, human_prompt, turn_type_id,
                        turn_origin_id, subagent_persona, created_at
                     )
                     OVERRIDING SYSTEM VALUE
                     SELECT $1,
                            (SELECT id FROM sessions.sessions WHERE session_uuid = $2),
                            $3, rm.id, '/v1/messages', false,
                            0, false, rs.id, '{}'::jsonb,
                            '', '', rtt.id,
                            rto.id, $4, $5
                     FROM requests.methods rm, requests.statuses rs,
                          requests.turn_types rtt, requests.turn_origins rto
                     WHERE rm.code = 'POST' AND rs.code = $6
                       AND rtt.code = $7 AND rto.code = $8",
                )
                .bind(self.id)
                .bind(session_uuid)
                .bind(self.project_id)
                .bind(self.subagent_persona)
                .bind(self.created_at)
                .bind(self.status)
                .bind(self.turn_type)
                .bind(self.turn_origin)
                .execute(pool)
                .await?;
            }
        }
        Ok(self.id)
    }
}
