use std::future::Future;

use anyhow::{Context, Result};

use crate::config::ProxyConfig;

pub(crate) struct RequestedBoot {
    pub(crate) listeners: ProxyConfig,
    pub(crate) persist_enabled: bool,
    pub(crate) database_url: Option<String>,
    pub(crate) local_database_url: Option<String>,
    pub(crate) db_fallback_enabled: bool,
}

#[derive(Debug)]
pub(crate) enum DatabaseFailure {
    Connection(anyhow::Error),
    Invalid(anyhow::Error),
}

impl DatabaseFailure {
    fn into_error(self) -> anyhow::Error {
        match self {
            Self::Connection(error) | Self::Invalid(error) => error,
        }
    }
}

pub(crate) async fn requested_boot_with<Probe, ProbeFuture, Validate, ValidateFuture>(
    config: &RequestedBoot,
    mut unreachable: Probe,
    mut validate: Validate,
) -> Result<()>
where
    Probe: FnMut(String) -> ProbeFuture,
    ProbeFuture: Future<Output = bool>,
    Validate: FnMut(String) -> ValidateFuture,
    ValidateFuture: Future<Output = Result<(), DatabaseFailure>>,
{
    crate::serve::listener_policy::validate(&config.listeners)?;
    if !config.persist_enabled {
        return Ok(());
    }
    let primary = config.database_url.as_deref().context(
        "persistence is enabled but no capture database is configured; set MIKE_DATABASE_URL or use MIKE_PERSIST_ENABLED=0",
    )?;
    let fallback = config
        .db_fallback_enabled
        .then_some(config.local_database_url.as_deref())
        .flatten();
    if let Some(local) = fallback
        && unreachable(primary.to_owned()).await
    {
        return validate(local.to_owned())
            .await
            .map_err(DatabaseFailure::into_error)
            .context("local fallback database is not ready; existing daemon was preserved");
    }
    match validate(primary.to_owned()).await {
        Ok(()) => Ok(()),
        Err(DatabaseFailure::Connection(_)) if fallback.is_some() => {
            validate(fallback.context("fallback database is missing")?.to_owned())
                .await
                .map_err(DatabaseFailure::into_error)
                .context("local fallback database is not ready; existing daemon was preserved")
        }
        Err(error) => Err(error.into_error())
            .context("primary database is not ready; existing daemon was preserved"),
    }
}

pub(crate) async fn after_preflight<T, Preflight, Change, ChangeFuture>(
    preflight: Preflight,
    change: Change,
) -> Result<T>
where
    Preflight: Future<Output = Result<()>>,
    Change: FnOnce() -> ChangeFuture,
    ChangeFuture: Future<Output = Result<T>>,
{
    preflight.await?;
    change().await
}

pub(crate) async fn bounded_validation<Validation>(
    timeout: std::time::Duration,
    validation: Validation,
) -> Result<(), DatabaseFailure>
where
    Validation: Future<Output = Result<(), DatabaseFailure>>,
{
    tokio::time::timeout(timeout, validation)
        .await
        .map_err(|_| {
            DatabaseFailure::Invalid(anyhow::anyhow!(
                "database preflight exceeded its deadline; existing daemon was preserved"
            ))
        })?
}

pub(crate) async fn verify_current_schema<Check, CheckFuture>(
    pending_migrations: usize,
    check: Check,
) -> Result<()>
where
    Check: FnOnce() -> CheckFuture,
    CheckFuture: Future<Output = Result<bool>>,
{
    if pending_migrations == 0 {
        anyhow::ensure!(
            check().await?,
            "mike schema is missing despite a current migration ledger; restore the database before restarting"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
