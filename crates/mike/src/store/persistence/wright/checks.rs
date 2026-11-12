use super::{CheckRow, NewCheck};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn list_wright_checks(&self) -> Result<Vec<CheckRow>, PersistError> {
        Ok(sqlx::query_as!(
            CheckRow,
            r#"SELECT name AS "name!", source AS "source!", cadence AS "cadence!",
                      expects AS "expects!", probe AS "probe!", query AS "query?",
                      params AS "params!", enabled AS "enabled!",
                      created_at AS "created_at!", updated_at AS "updated_at!"
                 FROM wright.checks
                ORDER BY name"#,
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub(crate) async fn enabled_wright_checks(
        &self,
        cadences: &[String],
    ) -> Result<Vec<CheckRow>, PersistError> {
        Ok(sqlx::query_as!(
            CheckRow,
            r#"SELECT name AS "name!", source AS "source!", cadence AS "cadence!",
                      expects AS "expects!", probe AS "probe!", query AS "query?",
                      params AS "params!", enabled AS "enabled!",
                      created_at AS "created_at!", updated_at AS "updated_at!"
                 FROM wright.checks
                WHERE enabled AND cadence = ANY($1)
                ORDER BY name"#,
            cadences,
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub(crate) async fn wright_check(&self, name: &str) -> Result<Option<CheckRow>, PersistError> {
        Ok(sqlx::query_as!(
            CheckRow,
            r#"SELECT name AS "name!", source AS "source!", cadence AS "cadence!",
                      expects AS "expects!", probe AS "probe!", query AS "query?",
                      params AS "params!", enabled AS "enabled!",
                      created_at AS "created_at!", updated_at AS "updated_at!"
                 FROM wright.checks
                WHERE name = $1"#,
            name,
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub(crate) async fn insert_wright_check(
        &self,
        check: &NewCheck,
    ) -> Result<Option<CheckRow>, PersistError> {
        Ok(sqlx::query_as!(
            CheckRow,
            r#"INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params, enabled)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (name) DO NOTHING
               RETURNING name AS "name!", source AS "source!", cadence AS "cadence!",
                         expects AS "expects!", probe AS "probe!", query AS "query?",
                         params AS "params!", enabled AS "enabled!",
                         created_at AS "created_at!", updated_at AS "updated_at!""#,
            check.name,
            check.source,
            check.cadence,
            check.expects,
            check.probe,
            check.query,
            check.params,
            check.enabled,
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub(crate) async fn replace_wright_check(
        &self,
        check: &NewCheck,
    ) -> Result<Option<CheckRow>, PersistError> {
        Ok(sqlx::query_as!(
            CheckRow,
            r#"UPDATE wright.checks
                  SET source = $2, cadence = $3, expects = $4, query = $5, params = $6,
                      enabled = $7, updated_at = now()
                WHERE name = $1
               RETURNING name AS "name!", source AS "source!", cadence AS "cadence!",
                         expects AS "expects!", probe AS "probe!", query AS "query?",
                         params AS "params!", enabled AS "enabled!",
                         created_at AS "created_at!", updated_at AS "updated_at!""#,
            check.name,
            check.source,
            check.cadence,
            check.expects,
            check.query,
            check.params,
            check.enabled,
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub(crate) async fn delete_wright_check(&self, name: &str) -> Result<bool, PersistError> {
        let deleted = sqlx::query_scalar!(
            r#"DELETE FROM wright.checks WHERE name = $1 RETURNING name AS "name!""#,
            name,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(deleted.is_some())
    }
}
