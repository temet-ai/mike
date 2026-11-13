use sqlx::PgConnection;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone)]
pub struct NewChange<'a> {
    pub subject_kind: &'a str,
    pub subject_key: &'a str,
    pub op: &'a str,
    pub old_value: Option<&'a serde_json::Value>,
    pub new_value: Option<&'a serde_json::Value>,
    pub source_json: &'a str,
}

pub async fn set_change_source(
    conn: &mut PgConnection,
    source_json: &str,
) -> Result<(), PersistError> {
    sqlx::query!(
        "SELECT set_config('mike.change_source', $1, true)",
        source_json
    )
    .fetch_one(conn)
    .await?;
    Ok(())
}

pub async fn own_change_ids(conn: &mut PgConnection) -> Result<Vec<i64>, PersistError> {
    let ids = sqlx::query_scalar!(
        r#"SELECT id AS "id!" FROM config.change_log
  WHERE xact_id = pg_current_xact_id()
  ORDER BY id"#
    )
    .fetch_all(conn)
    .await?;
    Ok(ids)
}

pub async fn append_change_in(
    conn: &mut PgConnection,
    change: &NewChange<'_>,
) -> Result<Vec<i64>, PersistError> {
    set_change_source(conn, change.source_json).await?;
    sqlx::query!(
        "SELECT config.append_change($1, $2, $3, $4, $5, false)",
        change.subject_kind,
        change.subject_key,
        change.op,
        change.old_value,
        change.new_value,
    )
    .execute(&mut *conn)
    .await?;
    own_change_ids(conn).await
}

impl PersistHandle {
    pub async fn append_change(&self, change: &NewChange<'_>) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("append_change", async move {
            let mut tx = self.pool.begin().await?;
            let ids = append_change_in(&mut tx, change).await?;
            tx.commit().await?;
            Ok(ids)
        })
        .await
    }

    pub async fn record_build_value(
        &self,
        build_value: &serde_json::Value,
        build_sha: &str,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("record_build_value", async move {
            let id = sqlx::query_file_scalar!(
                "src/store/persistence/change_log/record_build.sql",
                build_value,
                build_sha,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn record_build_if_changed(
        &self,
        build: crate::change_log::BuildStamp,
        defaults: serde_json::Value,
    ) -> Result<Option<i64>, PersistError> {
        let value = build.to_value(defaults);
        self.record_build_value(&value, build.sha).await
    }
}
