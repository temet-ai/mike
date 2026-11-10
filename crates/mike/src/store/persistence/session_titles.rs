use std::collections::HashMap;

use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn session_titles(
        &self,
        session_uuids: &[String],
    ) -> Result<HashMap<String, String>, PersistError> {
        let ids: Vec<Uuid> = session_uuids
            .iter()
            .filter_map(|id| Uuid::parse_str(id.trim()).ok())
            .collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        crate::metrics::time_db("session_titles", async {
            let mut query = QueryBuilder::<Postgres>::new(
                "SELECT session_uuid, title FROM sessions.sessions
                  WHERE title IS NOT NULL AND btrim(title) <> '' AND session_uuid = ANY(",
            );
            query.push_bind(ids).push(")");
            let rows: Vec<(Uuid, String)> = query.build_query_as().fetch_all(&self.pool).await?;
            Ok(rows
                .into_iter()
                .map(|(id, title)| (id.to_string(), title))
                .collect())
        })
        .await
    }
}
