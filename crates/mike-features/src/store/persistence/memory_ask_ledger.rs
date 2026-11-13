use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::PersistError;

pub const JC03_MIGRATION_VERSION: i64 = 20260924180000;

#[derive(Debug, Clone, Copy)]
pub struct NewAskLedgerEntry<'a> {
    pub session_id: i64,
    pub request_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub kind: &'a str,
    pub kind_source: &'a str,
    pub content: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AskLedgerInspectionRow {
    pub id: i64,
    pub session_id: i64,
    pub operator_turn_id: Option<i64>,
    pub request_id: Option<i64>,
    pub kind: String,
    pub kind_source: String,
    pub keyword_kind: Option<String>,
    pub kind_updated_at: Option<DateTime<Utc>>,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub genuine: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AskLedgerEntryRow {
    pub id: i64,
    pub session_id: i64,
    pub operator_turn_id: Option<i64>,
    pub kind: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn append_ask_ledger_entry(
        self,
        entry: &NewAskLedgerEntry<'_>,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("append_ask_ledger_entry", async move {
            let id = sqlx::query_scalar!(
                "INSERT INTO memory.ask_ledger_entries
        (session_id, operator_turn_id, request_id, kind, kind_source, keyword_kind, content)
 VALUES ($1, $2, $3, $4, $5, CASE WHEN $4 = 'redirect' THEN NULL ELSE $4 END, $6)
 ON CONFLICT (request_id) WHERE request_id IS NOT NULL DO NOTHING
 RETURNING id",
                entry.session_id,
                entry.operator_turn_id,
                entry.request_id,
                entry.kind,
                entry.kind_source,
                entry.content,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn fetch_ask_ledger(
        self,
        session_uuid: Uuid,
        limit: i64,
    ) -> Result<Vec<AskLedgerEntryRow>, PersistError> {
        crate::metrics::time_db("fetch_ask_ledger", async move {
            let Some(session_db_id) = self.session_row_id(session_uuid).await? else {
                return Ok(Vec::new());
            };
            let rows = sqlx::query_as!(
                AskLedgerEntryRow,
                r#"SELECT sub.id, sub.session_id, sub.operator_turn_id, sub.kind, sub.content, sub.created_at
                     FROM (
                         SELECT e.id, e.session_id, e.operator_turn_id, e.kind, e.content, e.created_at
                           FROM memory.ask_ledger_entries e
                          WHERE e.session_id = $1
                          ORDER BY e.created_at DESC
                          LIMIT $2
                     ) sub
                     ORDER BY sub.created_at ASC"#,
                session_db_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_ask_ledger_for_model(
        self,
        session_uuid: Uuid,
        limit: i64,
    ) -> Result<Vec<AskLedgerEntryRow>, PersistError> {
        crate::metrics::time_db("fetch_ask_ledger_for_model", async move {
            let Some(session_db_id) = self.session_row_id(session_uuid).await? else {
                return Ok(Vec::new());
            };
            let rows = sqlx::query_as!(
                AskLedgerEntryRow,
                r#"SELECT sub.id AS "id!", sub.session_id AS "session_id!",
          sub.operator_turn_id AS "operator_turn_id?", sub.kind AS "kind!",
          sub.content AS "content!", sub.created_at AS "created_at!"
   FROM (
       SELECT e.id, e.session_id, e.operator_turn_id, e.kind, e.content, e.created_at
         FROM memory.ask_ledger_entries e
         LEFT JOIN prompt_kinds.classifications c ON c.request_id = e.request_id
         LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
        WHERE e.session_id = $1
          AND (k.code IS NULL OR k.code = 'operator')
        ORDER BY e.created_at DESC
        LIMIT $2
   ) sub
  ORDER BY sub.created_at ASC"#,
                session_db_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn count_ask_ledger_for_model(self, session_uuid: Uuid) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_ask_ledger_for_model", async move {
            let Some(session_db_id) = self.session_row_id(session_uuid).await? else {
                return Ok(0);
            };
            let total = sqlx::query_scalar!(
                r#"SELECT count(*) AS "total!"
                     FROM memory.ask_ledger_entries e
                     LEFT JOIN prompt_kinds.classifications c ON c.request_id = e.request_id
                     LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
                    WHERE e.session_id = $1
                      AND (k.code IS NULL OR k.code = 'operator')"#,
                session_db_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(total)
        })
        .await
    }

    pub async fn fetch_ask_ledger_inspection(
        self,
        session_uuid: Uuid,
        limit: i64,
    ) -> Result<Vec<AskLedgerInspectionRow>, PersistError> {
        crate::metrics::time_db("fetch_ask_ledger_inspection", async move {
            let Some(session_db_id) = self.session_row_id(session_uuid).await? else {
                return Ok(Vec::new());
            };
            let rows = sqlx::query_as!(
                AskLedgerInspectionRow,
                r#"SELECT sub.id AS "id!", sub.session_id AS "session_id!",
              sub.operator_turn_id AS "operator_turn_id?", sub.request_id AS "request_id?",
              sub.kind AS "kind!", sub.kind_source AS "kind_source!",
              sub.keyword_kind AS "keyword_kind?", sub.kind_updated_at AS "kind_updated_at?",
              sub.content AS "content!", sub.created_at AS "created_at!",
              sub.genuine AS "genuine!"
   FROM (
       SELECT e.id, e.session_id, e.operator_turn_id, e.request_id, e.kind, e.kind_source,
              e.keyword_kind, e.kind_updated_at, e.content, e.created_at,
              (k.code IS NULL OR k.code = 'operator') AS genuine
         FROM memory.ask_ledger_entries e
         LEFT JOIN prompt_kinds.classifications c ON c.request_id = e.request_id
         LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
        WHERE e.session_id = $1
        ORDER BY e.created_at DESC
        LIMIT $2
   ) sub
  ORDER BY sub.created_at ASC"#,
                session_db_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn ledger_builder_since(self) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("ledger_builder_since", async move {
            let installed = sqlx::query_scalar!(
                "SELECT installed_on FROM _sqlx_migrations WHERE version = $1 AND success",
                JC03_MIGRATION_VERSION,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(installed)
        })
        .await
    }
}

#[cfg(test)]
pub mod tests {
    use super::JC03_MIGRATION_VERSION;

    #[test]
    fn jc03_migration_version_constant_matches_migration_file() {
        let name = format!("{JC03_MIGRATION_VERSION}_ask_ledger_dossier_entries.sql");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/store/persistence/migrations")
            .join(&name);
        assert!(path.exists(), "missing migration file {name}");
    }
}
