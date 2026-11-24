use serde::Serialize;
use uuid::Uuid;

use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Serialize)]
pub struct ProposedSessionProject {
    pub session_uuid: Uuid,
    pub rule: String,
    pub cwd: Option<String>,
    pub project_id: i64,
    pub project_path: String,
    pub location_uuid: Option<Uuid>,
    pub resolution: String,
}

impl PersistHandle {
    pub async fn propose_unresolved_session_projects(
        &self,
        wright_project: Option<i64>,
    ) -> Result<Vec<ProposedSessionProject>, PersistError> {
        let rows = sqlx::query_as!(
            ProposedSessionProject,
            r#"WITH unresolved AS (
                 SELECT s.session_uuid, s.cwd, s.machine_uuid
                   FROM sessions.sessions s
                  WHERE s.project_resolution = 'unresolved' AND s.location_uuid IS NULL
               ), scratch AS (
                 SELECT u.session_uuid, 1 AS priority, 'parent_scratchpad' AS rule, u.cwd,
                        parent.project_id, parent.location_uuid, parent.project_resolution AS resolution
                   FROM unresolved u
                   JOIN sessions.sessions parent
                     ON parent.session_uuid = substring(u.cwd from
                          '/claude-[0-9]+/[^/]+/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/scratchpad(/|$)')::uuid
                   JOIN projects.projects p ON p.id = parent.project_id
                  WHERE parent.project_resolution <> 'unresolved'
                    AND (starts_with(p.project_path, 'machine:') OR NOT EXISTS (
                      SELECT 1 FROM projects.locations pl
                        JOIN projects.locations held ON held.machine_uuid = pl.machine_uuid
                         AND held.project_id <> pl.project_id
                         AND starts_with(held.root_path, rtrim(pl.root_path, '/') || '/')
                       WHERE pl.project_id = p.id))
                    AND parent.session_uuid <> u.session_uuid
               ), candidates AS (
                 SELECT u.session_uuid, u.cwd, u.machine_uuid, l.project_id, l.location_uuid, l.root_path
                   FROM unresolved u
                   JOIN projects.locations l
                     ON (u.machine_uuid IS NULL OR l.machine_uuid = u.machine_uuid)
                   JOIN projects.projects lp ON lp.id = l.project_id
                    AND (rtrim(l.root_path, '/') = rtrim(u.cwd, '/')
                         OR (starts_with(u.cwd, rtrim(l.root_path, '/') || '/')
                             AND ((starts_with(lp.project_path, 'machine:')
                                   AND u.cwd !~ '(/\.cache/|/\.local/state/|/warm/cwd(/|$))'
                                   AND NOT EXISTS (
                                       SELECT 1 FROM projects.locations other
                                        WHERE other.machine_uuid = l.machine_uuid
                                          AND other.project_id <> l.project_id
                                          AND starts_with(other.root_path || '/',
                                                rtrim(l.root_path, '/') || '/'
                                                || split_part(substr(u.cwd, length(rtrim(l.root_path, '/')) + 2), '/', 1)
                                                || '/')))
                                  OR NOT EXISTS (
                                 SELECT 1 FROM projects.locations held
                                  WHERE held.machine_uuid = l.machine_uuid
                                    AND held.project_id <> l.project_id
                                    AND starts_with(held.root_path, rtrim(l.root_path, '/') || '/')))))
                  WHERE u.cwd IS NOT NULL
               ), unambiguous AS (
                 SELECT session_uuid FROM candidates
                  GROUP BY session_uuid, machine_uuid
                 HAVING machine_uuid IS NOT NULL OR count(DISTINCT project_id) = 1
               ), located AS (
                 SELECT DISTINCT ON (c.session_uuid) c.session_uuid, 2 AS priority, 'location' AS rule,
                        c.cwd, c.project_id, c.location_uuid, 'operator' AS resolution
                   FROM candidates c
                   JOIN unambiguous a ON a.session_uuid = c.session_uuid
                  ORDER BY c.session_uuid, length(c.root_path) DESC, c.location_uuid
               ), wright AS (
                 SELECT u.session_uuid, 3 AS priority, 'wright_engineer' AS rule, u.cwd,
                        $1::bigint AS project_id, main.location_uuid, 'operator' AS resolution
                   FROM unresolved u
                   JOIN wright.investigations i ON i.session_uuid = u.session_uuid
                   LEFT JOIN LATERAL (
                     SELECT l.location_uuid FROM projects.locations l
                      WHERE l.project_id = $1::bigint AND l.machine_uuid = i.machine_uuid
                        AND l.parent_location_uuid IS NULL AND l.gone_at IS NULL
                      ORDER BY length(l.root_path), l.location_uuid
                      LIMIT 1
                   ) main ON true
                  WHERE $1::bigint IS NOT NULL
               ), proposals AS (
                 SELECT * FROM scratch UNION ALL SELECT * FROM located UNION ALL SELECT * FROM wright
               )
               SELECT DISTINCT ON (x.session_uuid)
                      x.session_uuid AS "session_uuid!", x.rule AS "rule!", x.cwd,
                      p.id AS "project_id!", p.project_path AS "project_path!",
                      x.location_uuid, x.resolution AS "resolution!"
                 FROM proposals x
                 JOIN projects.projects named ON named.id = x.project_id
                 JOIN projects.projects p ON p.id = COALESCE(named.merged_into_id, named.id)
                ORDER BY x.session_uuid, x.priority"#,
            wright_project,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    pub async fn apply_proposed_session_project(
        &self,
        proposal: &ProposedSessionProject,
    ) -> Result<(), PersistError> {
        if let Some(location) = proposal.location_uuid {
            self.bind_session_location(proposal.session_uuid, location)
                .await?;
            return Ok(());
        }
        let mut tx = self.pool().begin().await?;
        let session_id = sqlx::query_scalar!(
            "SELECT id FROM sessions.sessions
              WHERE session_uuid = $1 AND project_resolution = 'unresolved'
              FOR NO KEY UPDATE",
            proposal.session_uuid,
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(session_id) = session_id {
            super::session_projects::repoint_session_project(
                &mut tx,
                session_id,
                proposal.project_id,
                &proposal.resolution,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "unresolved_session_backfill_db_tests.rs"]
mod db_tests;
