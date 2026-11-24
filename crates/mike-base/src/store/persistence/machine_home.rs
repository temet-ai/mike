use uuid::Uuid;

use super::{PersistError, PersistHandle};

pub const MACHINE_PROJECT_PREFIX: &str = "machine:";

pub fn machine_project_path(machine: Uuid) -> String {
    format!("{MACHINE_PROJECT_PREFIX}{machine}")
}

#[derive(Debug, Clone)]
pub struct HomeRoot {
    pub machine_uuid: Uuid,
    pub machine_name: String,
    pub home_root: String,
    pub sessions: i64,
}

impl PersistHandle {
    pub async fn ensure_machine_home_location(
        &self,
        machine: Uuid,
        root: &str,
    ) -> Result<Option<Uuid>, PersistError> {
        let location = sqlx::query_scalar!(
            r#"WITH m AS (
                 SELECT name FROM projects.machines WHERE machine_uuid = $1
               ), p AS (
                 INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
                 SELECT $2, m.name || ' home', now(), now() FROM m
                 ON CONFLICT (project_path) DO UPDATE SET last_seen = now()
                 RETURNING COALESCE(merged_into_id, id) AS id
               )
               INSERT INTO projects.locations (project_id, machine_uuid, root_path)
               SELECT p.id, $1, $3 FROM p
               ON CONFLICT (machine_uuid, root_path) DO UPDATE SET last_seen = now()
               RETURNING location_uuid"#,
            machine,
            machine_project_path(machine),
            root.trim_end_matches('/'),
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(location)
    }

    pub async fn folder_holds_other_projects(
        &self,
        machine: Uuid,
        folder: &str,
    ) -> Result<bool, PersistError> {
        let held = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                 SELECT 1 FROM projects.locations l
                   JOIN projects.projects p ON p.id = l.project_id
                  WHERE l.machine_uuid = $1
                    AND NOT starts_with(p.project_path, 'machine:')
                    AND starts_with(l.root_path || '/', rtrim($2, '/') || '/')
               ) AS "held!""#,
            machine,
            folder,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(held)
    }

    pub async fn unresolved_home_roots(&self) -> Result<Vec<HomeRoot>, PersistError> {
        let roots = sqlx::query_as!(
            HomeRoot,
            r#"WITH homed AS (
                 SELECT s.machine_uuid, s.project_resolution, s.location_uuid, s.cwd,
                        substring(s.cwd from '^(/home/[^/]+|/Users/[^/]+|/root)(/|$)') AS home_root
                   FROM sessions.sessions s
                  WHERE s.cwd IS NOT NULL
               ), known AS (
                 SELECT home_root, (array_agg(DISTINCT machine_uuid))[1] AS machine_uuid
                   FROM homed
                  WHERE home_root IS NOT NULL AND machine_uuid IS NOT NULL
                  GROUP BY home_root
                 HAVING count(DISTINCT machine_uuid) = 1
               ), placed AS (
                 SELECT COALESCE(h.machine_uuid, k.machine_uuid) AS machine_uuid, h.home_root
                   FROM homed h
                   LEFT JOIN known k ON k.home_root = h.home_root
                  WHERE h.home_root IS NOT NULL
                    AND h.cwd !~ '(/\.cache/|/\.local/state/|/warm/cwd(/|$))'
                    AND h.project_resolution = 'unresolved'
                    AND h.location_uuid IS NULL
                    AND NOT EXISTS (
                        SELECT 1 FROM projects.locations other
                          JOIN projects.projects op ON op.id = other.project_id
                         WHERE other.machine_uuid = COALESCE(h.machine_uuid, k.machine_uuid)
                           AND NOT starts_with(op.project_path, 'machine:')
                           AND starts_with(other.root_path || '/', h.home_root || '/'
                                 || split_part(substr(h.cwd, length(h.home_root) + 2), '/', 1) || '/'))
               )
               SELECT p.machine_uuid AS "machine_uuid!", m.name AS machine_name,
                      p.home_root AS "home_root!", count(*) AS "sessions!"
                 FROM placed p
                 JOIN projects.machines m ON m.machine_uuid = p.machine_uuid
                GROUP BY p.machine_uuid, m.name, p.home_root
                ORDER BY m.name, p.home_root"#,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(roots)
    }
}

#[cfg(test)]
#[path = "machine_home_db_tests.rs"]
mod db_tests;
