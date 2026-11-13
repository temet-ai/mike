#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error(
        "mike requires PostgreSQL 19; connected server is major {0}; upgrade the cluster before starting mike"
    )]
    UnsupportedServerVersion(i32),

    #[error("failed to connect to Postgres at {url}: {source}")]
    Connect {
        url: String,
        #[source]
        source: sqlx::Error,
    },

    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error(
        "database schema does not match this binary ({0}); this command does not migrate — \
         run `mike migrate-check`, and apply migrations only through `mike serve` of the \
         build that owns them"
    )]
    SchemaNotCurrent(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("data directory could not be resolved: {0}")]
    DataDir(String),

    #[error(
        "persistence is enabled but MIKE_DATABASE_URL is not set — provide a Postgres URL or set MIKE_PERSIST_ENABLED=0"
    )]
    MissingDatabaseUrl,

    #[error("JSON (de)serialization failed during persistence: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("persistence data invariant violated: {0}")]
    DataInvariant(String),

    #[error("invalid acceptance obligation: {0}")]
    InvalidAcceptanceObligation(String),

    #[error("invalid registered_via value: {0}")]
    InvalidRegisteredVia(String),

    #[error("no active workspace registration for session {0}")]
    RegistrationNotFound(uuid::Uuid),

    #[error("operator turn {0} does not exist")]
    TurnNotFound(i64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_database_url_display_is_actionable() {
        let msg = PersistError::MissingDatabaseUrl.to_string();
        assert!(msg.contains("MIKE_DATABASE_URL"));
        assert!(msg.contains("MIKE_PERSIST_ENABLED"));
    }
}
