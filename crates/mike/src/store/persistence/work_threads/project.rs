use uuid::Uuid;

use super::WorkThreadError;
use super::validate::invalid;
use crate::store::persistence::project_knowledge::ProjectIdentity;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn resolve_work_thread_project(
        self,
        selector: &str,
    ) -> Result<Option<ProjectIdentity>, WorkThreadError> {
        let selector = selector.trim();
        let rows = sqlx::query!(
            r#"SELECT c.id, c.project_uuid, c.project_path, m.rank AS "rank!"
               FROM projects.projects c
               JOIN (SELECT COALESCE(p.merged_into_id, p.id) AS id,
                            MIN(CASE WHEN p.project_uuid = $1 THEN 0
                                     WHEN l.root_path = $2 THEN 1
                                     ELSE 2 END) AS rank
                       FROM projects.projects p
                       LEFT JOIN projects.locations l ON l.project_id = p.id
                      WHERE p.project_uuid = $1 OR p.project_path = $2 OR l.root_path = $2
                      GROUP BY 1) m ON m.id = c.id
              WHERE c.merged_into_id IS NULL
              ORDER BY m.rank, c.id"#,
            selector.parse::<Uuid>().ok(),
            selector,
        )
        .fetch_all(self.pool())
        .await?;
        let best = rows.first().map(|row| row.rank);
        let rows: Vec<ProjectIdentity> = rows
            .into_iter()
            .filter(|row| Some(row.rank) == best)
            .map(|row| ProjectIdentity {
                id: row.id,
                project_uuid: row.project_uuid,
                project_path: row.project_path,
            })
            .collect();
        if rows.len() > 1 {
            let paths: Vec<&str> = rows.iter().map(|row| row.project_path.as_str()).collect();
            return Err(invalid(format!(
                "project `{selector}` matches {} projects ({}); use its project UUID",
                rows.len(),
                paths.join(", ")
            )));
        }
        Ok(rows.into_iter().next())
    }
}
