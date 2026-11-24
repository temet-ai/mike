use uuid::Uuid;

use super::project_identity::ProjectIdentity;
use super::{PersistError, PersistHandle};

pub fn scratchpad_parent_session(cwd: &str) -> Option<Uuid> {
    let segments: Vec<&str> = cwd.split('/').collect();
    segments.windows(4).find_map(|window| {
        let owner = window[0].strip_prefix("claude-")?;
        let numeric = !owner.is_empty() && owner.bytes().all(|b| b.is_ascii_digit());
        if !numeric || window[3] != "scratchpad" {
            return None;
        }
        Uuid::parse_str(window[2]).ok()
    })
}

struct ParentProject {
    child_id: i64,
    project_id: i64,
    project_uuid: Uuid,
    project_path: String,
    resolution: String,
    location_uuid: Option<Uuid>,
}

impl PersistHandle {
    pub async fn adopt_parent_session_project(
        &self,
        session: Uuid,
        parent: Uuid,
    ) -> Result<Option<ProjectIdentity>, PersistError> {
        if session == parent {
            return Ok(None);
        }
        let Some(found) = self.parent_project(session, parent).await? else {
            return Ok(None);
        };
        if let Some(location) = found.location_uuid {
            return self.bind_session_location(session, location).await.map(Some);
        }
        let mut tx = self.pool().begin().await?;
        super::session_projects::repoint_session_project(
            &mut tx,
            found.child_id,
            found.project_id,
            &found.resolution,
        )
        .await?;
        tx.commit().await?;
        Ok(Some(ProjectIdentity {
            id: found.project_id,
            project_uuid: found.project_uuid,
            project_path: found.project_path,
        }))
    }

    async fn parent_project(
        &self,
        session: Uuid,
        parent: Uuid,
    ) -> Result<Option<ParentProject>, PersistError> {
        let row = sqlx::query_as!(
            ParentProject,
            r#"SELECT child.id AS child_id, p.id AS project_id, p.project_uuid, p.project_path,
                      parent.project_resolution AS resolution, parent.location_uuid
                 FROM sessions.sessions child
                 JOIN sessions.sessions parent ON parent.session_uuid = $2
                 JOIN projects.projects p ON p.id = parent.project_id
                WHERE child.session_uuid = $1
                  AND child.project_resolution <> 'operator'
                  AND child.location_uuid IS NULL
                  AND parent.project_resolution <> 'unresolved'
                  AND (starts_with(p.project_path, 'machine:') OR NOT EXISTS (
                      SELECT 1 FROM projects.locations pl
                        JOIN projects.locations held ON held.machine_uuid = pl.machine_uuid
                         AND held.project_id <> pl.project_id
                         AND starts_with(held.root_path, rtrim(pl.root_path, '/') || '/')
                       WHERE pl.project_id = p.id))"#,
            session,
            parent,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
#[path = "session_parent_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "session_parent_db_tests.rs"]
pub(crate) mod db_tests;
