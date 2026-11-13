use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::super::WorkThreadError;

#[derive(Debug, Clone)]
pub struct OpenCardRow {
    pub thread_id: i64,
    pub project_id: i64,
    pub slug: String,
    pub title: String,
    pub goal: String,
    pub status: String,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub last_session_uuid: Option<Uuid>,
    pub last_checkpoint_at: DateTime<Utc>,
    pub text: String,
    pub content_hash: String,
    pub linked_sessions: Vec<Uuid>,
    pub checkout_dirs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LocationRoot {
    pub root_path: String,
    pub project_id: i64,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn thread_cards_generation(self) -> Result<i64, WorkThreadError> {
        let generation = crate::metrics::time_db("thread_cards_generation", async move {
            sqlx::query_scalar!(
                r#"SELECT generation AS "generation!"
                     FROM memory.change_generations
                    WHERE name = 'thread_cards'"#
            )
            .fetch_one(self.pool())
            .await
        })
        .await?;
        Ok(generation)
    }

    pub async fn open_thread_cards(self) -> Result<Vec<OpenCardRow>, WorkThreadError> {
        let rows = crate::metrics::time_db("open_thread_cards", async move {
            sqlx::query_as!(
                OpenCardRow,
                r#"WITH open_threads AS (
                       SELECT t.id, t.project_id, t.slug, COALESCE(t.title, t.slug) AS title,
                              COALESCE(t.goal, '') AS goal, t.status, t.branch, t.worktree,
                              t.last_session_uuid,
                              GREATEST(t.updated_at, s.updated_at) AS checkpoint_at
                         FROM work.threads t
                         LEFT JOIN work.thread_state s ON s.thread_id = t.id
                        WHERE t.status IN ('active', 'implementing', 'review', 'in-progress')
                   ), linked AS (
                       SELECT x.thread_id,
                              array_agg(DISTINCT x.session_uuid) AS sessions,
                              array_agg(DISTINCT ss.cwd) FILTER (WHERE ss.cwd IS NOT NULL) AS dirs
                         FROM work.thread_sessions x
                         JOIN open_threads o ON o.id = x.thread_id
                         LEFT JOIN sessions.sessions ss ON ss.session_uuid = x.session_uuid
                        GROUP BY x.thread_id
                   )
                   SELECT o.id AS "thread_id!",
                          o.project_id AS "project_id!",
                          o.slug AS "slug!",
                          o.title AS "title!",
                          o.goal AS "goal!",
                          o.status AS "status!",
                          o.branch,
                          o.worktree,
                          o.last_session_uuid,
                          o.checkpoint_at AS "last_checkpoint_at!",
                          c.text AS "text!",
                          c.content_hash AS "content_hash!",
                          COALESCE(l.sessions, '{}') AS "linked_sessions!",
                          COALESCE(l.dirs, '{}') AS "checkout_dirs!"
                     FROM open_threads o
                     JOIN work.thread_cards c ON c.thread_id = o.id
                     LEFT JOIN linked l ON l.thread_id = o.id
                    ORDER BY o.project_id, o.slug"#
            )
            .fetch_all(self.pool())
            .await
        })
        .await?;
        Ok(rows)
    }

    pub async fn location_roots(self) -> Result<Vec<LocationRoot>, WorkThreadError> {
        let rows = crate::metrics::time_db("location_roots", async move {
            sqlx::query_as!(
                LocationRoot,
                r#"SELECT DISTINCT root_path AS "root_path!", project_id AS "project_id!"
                     FROM projects.locations
                    WHERE root_path <> ''"#
            )
            .fetch_all(self.pool())
            .await
        })
        .await?;
        Ok(rows)
    }
}
