use super::meta;
use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn pin_session(&self, session_id: &str) -> Result<(), PersistError> {
        crate::metrics::time_db("pin_session", async move {
            let db_id = self.ensure_session_exists(session_id).await?;
            sqlx::query!(
                "UPDATE sessions.sessions
                 SET pinned = TRUE,
                     pin_created_at = COALESCE(pin_created_at, NOW())
                 WHERE id = $1",
                db_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub async fn unpin_session(&self, session_db_id: i64) -> Result<(), PersistError> {
        crate::metrics::time_db("unpin_session", async move {
            sqlx::query!(
                "UPDATE sessions.sessions
                 SET pinned = FALSE, pin_created_at = NULL
                 WHERE id = $1",
                session_db_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub async fn set_session_ttl(
        &self,
        session_id: &str,
        ttl: Option<std::time::Duration>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("set_session_ttl", async move {
            let db_id = self.ensure_session_exists(session_id).await?;
            let ttl_pg: Option<sqlx::postgres::types::PgInterval> =
                ttl.map(|d| sqlx::postgres::types::PgInterval {
                    months: 0,
                    days: 0,
                    microseconds: d.as_micros() as i64,
                });
            let has_ttl = ttl_pg.is_some();
            sqlx::query!(
                "UPDATE sessions.sessions
                 SET ttl_override = $2,
                     ttl_override_set_at = CASE WHEN $3 THEN NOW() ELSE NULL END
                 WHERE id = $1",
                db_id,
                ttl_pg as Option<sqlx::postgres::types::PgInterval>,
                has_ttl,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub async fn pin_project(&self, project_path: &str) -> Result<(), PersistError> {
        let project_path = project_path.to_owned();
        let display_name = crate::extract::request::project_display_name(
            crate::extract::request::compute_project_root(&project_path),
        );
        crate::metrics::time_db("pin_project", async move {
            sqlx::query!(
                "INSERT INTO projects.projects
                    (project_path, display_name, first_seen, last_seen, pinned, pin_created_at)
                 VALUES ($1, $2, NOW(), NOW(), TRUE, NOW())
                 ON CONFLICT (project_path) DO UPDATE
                 SET pinned = TRUE,
                     pin_created_at = COALESCE(projects.projects.pin_created_at, NOW()),
                     last_seen = NOW()",
                project_path.as_str(),
                display_name.as_str(),
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub async fn unpin_project(&self, project_path: &str) -> Result<(), PersistError> {
        let project_path = project_path.to_owned();
        crate::metrics::time_db("unpin_project", async move {
            sqlx::query!(
                "UPDATE projects.projects
                 SET pinned = FALSE, pin_created_at = NULL
                 WHERE project_path = $1",
                project_path,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub(crate) async fn set_project_custom_name(
        &self,
        project_path: &str,
        custom_name: Option<&str>,
    ) -> Result<bool, PersistError> {
        let project_path = project_path.to_owned();
        crate::metrics::time_db("set_project_custom_name", async move {
            let result = sqlx::query!(
                "UPDATE projects.projects
                 SET custom_name = $2
                 WHERE project_path = $1",
                project_path,
                custom_name,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }
    pub async fn set_project_ttl(
        &self,
        project_path: &str,
        ttl: Option<std::time::Duration>,
    ) -> Result<(), PersistError> {
        let project_path = project_path.to_owned();
        let display_name = crate::extract::request::project_display_name(
            crate::extract::request::compute_project_root(&project_path),
        );
        crate::metrics::time_db("set_project_ttl", async move {
            sqlx::query!(
                "INSERT INTO projects.projects
                    (project_path, display_name, first_seen, last_seen, ttl_override, ttl_override_set_at)
                 VALUES ($1, $3, NOW(), NOW(), $2::interval,
                         CASE WHEN $2::interval IS NULL THEN NULL ELSE NOW() END)
                 ON CONFLICT (project_path) DO UPDATE
                 SET ttl_override = EXCLUDED.ttl_override,
                     ttl_override_set_at = EXCLUDED.ttl_override_set_at,
                     last_seen = NOW()",
                project_path.as_str(),
                ttl as _,
                display_name.as_str(),
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub async fn effective_ttl_for_session(
        &self,
        session_db_id: i64,
        default_ttl: std::time::Duration,
    ) -> Result<std::time::Duration, PersistError> {
        crate::metrics::time_db("effective_ttl_for_session", async move {
            let interval = sqlx::query_scalar!(
                r#"SELECT COALESCE(s.ttl_override, p.ttl_override) AS "ttl?"
                 FROM sessions.sessions s
                 LEFT JOIN projects.projects p ON p.id = s.project_id
                 WHERE s.id = $1"#,
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            let ttl_str = interval.flatten().and_then(meta::pg_interval_to_humantime);
            match ttl_str {
                Some(s) => Ok(crate::store::retention::RetentionPolicy::parse_duration(&s)),
                None => Ok(default_ttl),
            }
        })
        .await
    }
    #[cfg(any(test, feature = "test-seams"))]
    pub async fn effective_pinned_for_session(
        &self,
        session_db_id: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("effective_pinned_for_session", async move {
            let pinned = sqlx::query_scalar!(
                r#"SELECT (s.pinned OR COALESCE(p.pinned, FALSE)) AS "pinned!"
                 FROM sessions.sessions s
                 LEFT JOIN projects.projects p ON p.id = s.project_id
                 WHERE s.id = $1"#,
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(pinned.unwrap_or(false))
        })
        .await
    }
    pub async fn ensure_session_exists(&self, session_id: &str) -> Result<i64, PersistError> {
        self.cached_session_or_ensure(session_id, self.insert_or_find_session(session_id))
            .await
    }

    async fn insert_or_find_session(&self, session_id: &str) -> Result<i64, PersistError> {
        crate::metrics::time_db("ensure_session_exists", async move {
            super::id_resolvers::resolve_session_id(&self.pool, session_id, None, None).await
        })
        .await
    }
}
