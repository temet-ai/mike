use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::{LaunchListFilter, LaunchRow};
use crate::store::persistence::PersistError;

fn list_launches_builder(filter: &LaunchListFilter) -> QueryBuilder<Postgres> {
    let mut builder = QueryBuilder::new(include_str!("list_launches_select.sql"));
    let mut keyword = " WHERE ";
    if let Some(run_id) = &filter.run_id {
        builder.push(keyword).push("l.run_id = ").push_bind(run_id);
        keyword = " AND ";
    }
    if filter.open_only {
        builder.push(keyword).push("l.closed_at IS NULL");
        keyword = " AND ";
    }
    if let Some(backend) = &filter.backend {
        builder
            .push(keyword)
            .push("l.backend = ")
            .push_bind(backend);
    }
    builder
        .push(" ORDER BY l.created_at DESC, l.id DESC LIMIT ")
        .push_bind(filter.limit);
    builder
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn find_launch_by_session(
        self,
        session_uuid: Uuid,
    ) -> Result<Option<LaunchRow>, PersistError> {
        crate::metrics::time_db("launch_find_launch_by_session", async move {
            let row = sqlx::query_file_as!(
                LaunchRow,
                "src/store/persistence/launch/find_launch_by_session.sql",
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn list_launches(
        self,
        filter: &LaunchListFilter,
    ) -> Result<Vec<LaunchRow>, PersistError> {
        crate::metrics::time_db("launch_list_launches", async move {
            let rows = list_launches_builder(filter)
                .build_query_as::<LaunchRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn count_open_launches(self, backend: &str) -> Result<i64, PersistError> {
        crate::metrics::time_db("launch_count_open_launches", async move {
            let count = sqlx::query_file_scalar!(
                "src/store/persistence/launch/count_open_launches.sql",
                backend,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(count)
        })
        .await
    }

    pub async fn session_has_remote_control_channel(
        self,
        session_uuid: Uuid,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("launch_session_has_remote_control_channel", async move {
            let present = sqlx::query_file_scalar!(
                "src/store/persistence/launch/session_has_channel.sql",
                session_uuid,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(present)
        })
        .await
    }

    pub async fn session_prompt_turn_seen(
        self,
        session_uuid: Uuid,
        since: chrono::DateTime<chrono::Utc>,
        prompt_prefix: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("launch_session_prompt_turn_seen", async move {
            let seen = sqlx::query_file_scalar!(
                "src/store/persistence/launch/prompt_turn_seen.sql",
                session_uuid,
                since,
                prompt_prefix,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(seen)
        })
        .await
    }
}
