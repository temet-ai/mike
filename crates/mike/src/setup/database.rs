use std::time::Duration;

use anyhow::{Result, bail, ensure};
use sqlx::PgPool;

use crate::store::persistence::{PersistError, migrations, pool};

mod assessment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseReadiness {
    pub server_major: i32,
    pub pending_migrations: usize,
}

pub async fn validate_existing(database_url: &str) -> Result<DatabaseReadiness> {
    validate_url(database_url)?;
    tokio::time::timeout(Duration::from_secs(10), validate(database_url))
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "database validation exceeded ten seconds; check connectivity and retry"
            )
        })?
}

pub fn validate_url(database_url: &str) -> Result<()> {
    let url = url::Url::parse(database_url).map_err(|_| {
        anyhow::anyhow!("provide a postgres:// or postgresql:// URL with a host, user and database")
    })?;
    ensure!(
        matches!(url.scheme(), "postgres" | "postgresql")
            && url.host_str().is_some()
            && !url.username().is_empty()
            && url.path().len() > 1,
        "provide a postgres:// or postgresql:// URL with a host, user and database"
    );
    ensure!(
        url.fragment().is_none(),
        "database URLs must not contain a fragment"
    );
    Ok(())
}

async fn validate(database_url: &str) -> Result<DatabaseReadiness> {
    let connection = pool::build_read_only_pool(database_url).await.map_err(|error| {
        match error {
            error @ PersistError::UnsupportedServerVersion(_) => anyhow::Error::new(error),
            _ => anyhow::anyhow!("database authentication or connection failed; check the host, database, user and credential"),
        }
    })?;
    let result = assess(&connection).await;
    connection.close().await;
    result
}

pub(super) async fn await_managed(database_url: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut backoff = Duration::from_millis(250);
        loop {
            match pool::build_read_only_pool(database_url).await {
                Ok(connection) => {
                    let result = assess(&connection).await;
                    connection.close().await;
                    return result.map(|_| ());
                }
                Err(error @ PersistError::UnsupportedServerVersion(_)) => return Err(error.into()),
                Err(_) => {}
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(4));
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("managed database authentication/readiness exceeded sixty seconds; inspect the owned database and retry; the volume and credential were preserved"))?
}

pub(crate) async fn assess(connection: &PgPool) -> Result<DatabaseReadiness> {
    let facts = assessment::read(connection).await.map_err(|_| {
        anyhow::anyhow!("database permission assessment failed; the role must be able to read PostgreSQL system catalogs")
    })?;
    facts.validate()?;
    if facts.has_ledger {
        assessment::require_successful_ledger(connection).await?;
    }
    let migrations = migrations::assess(connection).await.map_err(|_| {
        anyhow::anyhow!(
            "migration ledger could not be read; restore its permissions or choose a fresh database"
        )
    })?;
    if migrations.is_blocking() {
        bail!(
            "migration ledger is newer than, changed from, or incompatible with this binary; use the matching mike version and review migrate-check before starting"
        );
    }
    assessment::extensions(connection, facts.superuser, migrations.pending.is_empty()).await?;
    Ok(DatabaseReadiness {
        server_major: facts.server_major,
        pending_migrations: migrations.pending.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation_rejects_invalid_and_incomplete_targets_without_echoing_secrets() -> Result<()>
    {
        for url in [
            "password-secret",
            "postgres://host/db",
            "https://user:secret@host/db",
            "postgres://user:secret@host/",
        ] {
            let error = validate_url(url)
                .err()
                .ok_or_else(|| anyhow::anyhow!("invalid URL must be rejected"))?;
            assert!(!error.to_string().contains("secret"));
        }
        Ok(())
    }

    #[test]
    fn url_validation_accepts_explicit_database_and_user() -> Result<()> {
        validate_url("postgres://mike:example@127.0.0.1:6666/mike")?;
        validate_url("postgresql://mike@localhost/mike?sslmode=require")
    }

    #[tokio::test]
    async fn unreachable_database_fails_without_disclosing_the_url() -> Result<()> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let result = validate_existing(&format!(
            "postgres://mike:private-password@127.0.0.1:{port}/missing"
        ))
        .await;
        let error = result
            .err()
            .ok_or_else(|| anyhow::anyhow!("unused port must fail"))?
            .to_string();
        assert!(!error.contains("private-password"));
        assert!(!error.contains("postgres://"));
        Ok(())
    }
}
