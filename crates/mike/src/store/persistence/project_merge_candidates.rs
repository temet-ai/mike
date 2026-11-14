use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{PersistError, SENTINEL_PROJECT_PATH};

#[derive(Debug, Clone)]
pub(crate) struct MergeCandidate {
    pub(crate) id: i64,
    pub(crate) project_uuid: Uuid,
    pub(crate) project_path: String,
    pub(crate) display_name: String,
    pub(crate) last_seen: DateTime<Utc>,
    pub(crate) origin_urls: Vec<String>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn merge_candidates(
        self,
        limit: i64,
    ) -> Result<Vec<MergeCandidate>, PersistError> {
        crate::metrics::time_db("merge_candidates", async move {
            Ok(sqlx::query_as!(
                MergeCandidate,
                r#"SELECT p.id AS "id!", p.project_uuid AS "project_uuid!",
                          p.project_path AS "project_path!", p.display_name AS "display_name!",
                          p.last_seen AS "last_seen!",
                          COALESCE(array_agg(DISTINCT l.origin_url)
                                   FILTER (WHERE l.origin_url IS NOT NULL), '{}')
                              AS "origin_urls!: Vec<String>"
                   FROM projects.projects p
                   LEFT JOIN projects.locations l ON l.project_id = p.id
                   WHERE p.merged_into_id IS NULL AND p.project_path <> $1
                   GROUP BY p.id
                   ORDER BY p.last_seen DESC
                   LIMIT $2"#,
                SENTINEL_PROJECT_PATH,
                limit,
            )
            .fetch_all(self.pool())
            .await?)
        })
        .await
    }
}
