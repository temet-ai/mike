use std::env;
use std::time::Duration;

use sqlx::ConnectOptions;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgConnection, PgPoolOptions};

use super::error::PersistError;

pub const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);
pub const SERVER_MAJOR: i32 = 19;

pub async fn require_server_version<'e, E: sqlx::PgExecutor<'e>>(
    executor: E,
) -> Result<(), PersistError> {
    let major = sqlx::query_scalar!(
        r#"SELECT current_setting('server_version_num')::int / 10000 AS "major!""#
    )
    .fetch_one(executor)
    .await?;
    if major != SERVER_MAJOR {
        return Err(PersistError::UnsupportedServerVersion(major));
    }
    Ok(())
}

const MIGRATION_LOCK_TIMEOUT_ENV: &str = "MIKE_MIGRATION_LOCK_TIMEOUT_SECS";
const DEFAULT_MIGRATION_LOCK_TIMEOUT_SECS: u64 = 180;

pub fn migration_lock_timeout_secs() -> u64 {
    env::var(MIGRATION_LOCK_TIMEOUT_ENV)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(DEFAULT_MIGRATION_LOCK_TIMEOUT_SECS)
}

pub async fn build_migration_connection(
    database_url: &str,
) -> Result<PgConnection, PersistError> {
    let parsed: PgConnectOptions =
        database_url
            .parse()
            .map_err(|e: sqlx::Error| PersistError::Connect {
                url: redact_url(database_url),
                source: e,
            })?;

    let lock_timeout_secs = migration_lock_timeout_secs();
    let lock_timeout = format!("{lock_timeout_secs}s");
    let options = parsed.options([
        ("application_name", "claude-proxy-migrate"),
        ("statement_timeout", "0"),
        ("lock_timeout", lock_timeout.as_str()),
    ]);

    options.connect().await.map_err(|e| PersistError::Connect {
        url: redact_url(database_url),
        source: e,
    })
}

pub async fn build_pool(database_url: &str) -> Result<PgPool, PersistError> {
    build_pool_with_timeout(database_url, DEFAULT_ACQUIRE_TIMEOUT).await
}

pub async fn build_pool_with_timeout(
    database_url: &str,
    acquire_timeout: Duration,
) -> Result<PgPool, PersistError> {
    build_pool_with_mode(database_url, acquire_timeout, false).await
}

pub async fn build_read_only_pool(database_url: &str) -> Result<PgPool, PersistError> {
    build_read_only_pool_with_timeout(database_url, DEFAULT_ACQUIRE_TIMEOUT).await
}

pub async fn build_read_only_pool_with_timeout(
    database_url: &str,
    acquire_timeout: Duration,
) -> Result<PgPool, PersistError> {
    build_pool_with_mode(database_url, acquire_timeout, true).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolShape {
    pub max_connections: u32,
    pub acquire_timeout: Duration,
    pub statement_timeout: Duration,
    pub read_only: bool,
}

const SINGLE_POOL_MAX_CONNECTIONS: u32 = 8;
const SINGLE_POOL_STATEMENT_TIMEOUT: Duration = Duration::from_secs(30);

impl PoolShape {
    fn single(acquire_timeout: Duration, read_only: bool) -> Self {
        Self {
            max_connections: SINGLE_POOL_MAX_CONNECTIONS,
            acquire_timeout,
            statement_timeout: SINGLE_POOL_STATEMENT_TIMEOUT,
            read_only,
        }
    }
}

pub fn connect_error(database_url: &str, source: sqlx::Error) -> PersistError {
    PersistError::Connect {
        url: redact_url(database_url),
        source,
    }
}

pub fn shaped_connect_options(
    database_url: &str,
    shape: &PoolShape,
) -> Result<PgConnectOptions, PersistError> {
    let parsed: PgConnectOptions = database_url
        .parse()
        .map_err(|e: sqlx::Error| connect_error(database_url, e))?;
    let statement_timeout = format!("{}ms", shape.statement_timeout.as_millis());
    let mut settings = vec![
        ("application_name", "claude-proxy"),
        ("statement_timeout", statement_timeout.as_str()),
        ("idle_in_transaction_session_timeout", "60s"),
        ("lock_timeout", "5s"),
        ("jit", "off"),
        ("plan_cache_mode", "force_custom_plan"),
    ];
    if shape.read_only {
        settings.push(("default_transaction_read_only", "on"));
    }
    Ok(parsed.options(settings).statement_cache_capacity(500))
}

pub fn shaped_pool_options(shape: &PoolShape) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(shape.max_connections.max(1))
        .min_connections(0)
        .acquire_timeout(shape.acquire_timeout)
        .idle_timeout(Some(Duration::from_secs(300)))
}

async fn build_pool_with_mode(
    database_url: &str,
    acquire_timeout: Duration,
    read_only: bool,
) -> Result<PgPool, PersistError> {
    let shape = PoolShape::single(acquire_timeout, read_only);
    let options = shaped_connect_options(database_url, &shape)?;
    let pool = shaped_pool_options(&shape)
        .connect_with(options)
        .await
        .map_err(|e| connect_error(database_url, e))?;

    require_server_version(&pool).await?;

    Ok(pool)
}

fn redact_url(raw: &str) -> String {
    let (scheme, rest) = match raw.split_once("://") {
        Some(pair) => pair,
        None => return raw.to_string(),
    };
    let sanitized_rest = match rest.split_once('@') {
        Some((_, host_part)) => format!("***@{host_part}"),
        None => rest.to_string(),
    };
    format!("{scheme}://{sanitized_rest}")
}

#[cfg(test)]
mod tests {
    use sqlx::Connection;

    use super::*;

    #[test]
    fn redact_url_masks_userinfo() {
        assert_eq!(
            redact_url("postgres://alice:hunter2@localhost:5432/proxy"),
            "postgres://***@localhost:5432/proxy"
        );
    }

    #[test]
    fn redact_url_preserves_when_no_credentials() {
        assert_eq!(
            redact_url("postgres://localhost:5432/proxy"),
            "postgres://localhost:5432/proxy"
        );
    }

    #[test]
    fn redact_url_preserves_unknown_scheme_layouts() {
        assert_eq!(redact_url("not a url"), "not a url");
    }

    #[tokio::test]
    async fn build_pool_fails_on_unreachable_host() -> anyhow::Result<()> {
        let url = "postgres://proxy_test:proxy_test@127.0.0.1:1/claude_proxy_never";
        let result = build_pool_with_timeout(url, Duration::from_millis(200)).await;
        match result {
            Err(PersistError::Connect { url: redacted, .. }) => {
                assert!(!redacted.contains("proxy_test:proxy_test"));
                assert!(redacted.starts_with("postgres://"));
                Ok(())
            }
            Err(other) => anyhow::bail!("expected Connect, got {other:?}"),
            Ok(_) => anyhow::bail!("expected error, got pool"),
        }
    }

    #[tokio::test]
    async fn build_pool_applies_session_gucs() -> anyhow::Result<()> {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            return Ok(());
        };

        let pool = build_pool(&db.database_url).await?;

        let app_name = sqlx::query_scalar!(r#"SELECT current_setting('application_name') AS "v!""#)
            .fetch_one(&pool)
            .await?;
        assert_eq!(app_name, "claude-proxy");

        let stmt_timeout =
            sqlx::query_scalar!(r#"SELECT current_setting('statement_timeout') AS "v!""#)
                .fetch_one(&pool)
                .await?;
        assert!(
            stmt_timeout == "30s" || stmt_timeout == "30000ms",
            "statement_timeout reported as {stmt_timeout:?} — expected 30s/30000ms"
        );

        let idle_tx = sqlx::query_scalar!(
            r#"SELECT current_setting('idle_in_transaction_session_timeout') AS "v!""#
        )
        .fetch_one(&pool)
        .await?;
        assert!(
            idle_tx == "1min" || idle_tx == "60s" || idle_tx == "60000ms",
            "idle_in_transaction_session_timeout reported as {idle_tx:?}"
        );

        let lock_timeout = sqlx::query_scalar!(r#"SELECT current_setting('lock_timeout') AS "v!""#)
            .fetch_one(&pool)
            .await?;
        assert!(
            lock_timeout == "5s" || lock_timeout == "5000ms",
            "lock_timeout reported as {lock_timeout:?} — expected 5s/5000ms"
        );

        pool.close().await;
        db.cleanup().await?;
        Ok(())
    }

    #[test]
    fn migration_lock_timeout_secs_defaults_when_env_unset() {
        unsafe {
            env::remove_var(MIGRATION_LOCK_TIMEOUT_ENV);
        }
        assert_eq!(
            migration_lock_timeout_secs(),
            DEFAULT_MIGRATION_LOCK_TIMEOUT_SECS
        );
    }

    #[test]
    fn migration_lock_timeout_secs_honors_env_override() {
        unsafe {
            env::set_var(MIGRATION_LOCK_TIMEOUT_ENV, "42");
        }
        assert_eq!(migration_lock_timeout_secs(), 42);
        unsafe {
            env::remove_var(MIGRATION_LOCK_TIMEOUT_ENV);
        }
    }

    #[test]
    fn migration_lock_timeout_secs_ignores_zero_and_unparseable() {
        unsafe {
            env::set_var(MIGRATION_LOCK_TIMEOUT_ENV, "0");
        }
        assert_eq!(
            migration_lock_timeout_secs(),
            DEFAULT_MIGRATION_LOCK_TIMEOUT_SECS
        );

        unsafe {
            env::set_var(MIGRATION_LOCK_TIMEOUT_ENV, "not-a-number");
        }
        assert_eq!(
            migration_lock_timeout_secs(),
            DEFAULT_MIGRATION_LOCK_TIMEOUT_SECS
        );

        unsafe {
            env::remove_var(MIGRATION_LOCK_TIMEOUT_ENV);
        }
    }

    #[tokio::test]
    async fn build_migration_connection_reports_patient_timeouts() -> anyhow::Result<()> {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            return Ok(());
        };

        unsafe {
            env::set_var(MIGRATION_LOCK_TIMEOUT_ENV, "91");
        }
        let mut conn = build_migration_connection(&db.database_url).await?;
        unsafe {
            env::remove_var(MIGRATION_LOCK_TIMEOUT_ENV);
        }

        let app_name = sqlx::query_scalar!(r#"SELECT current_setting('application_name') AS "v!""#)
            .fetch_one(&mut conn)
            .await?;
        assert_eq!(app_name, "claude-proxy-migrate");

        let stmt_timeout =
            sqlx::query_scalar!(r#"SELECT current_setting('statement_timeout') AS "v!""#)
                .fetch_one(&mut conn)
                .await?;
        assert_eq!(
            stmt_timeout, "0",
            "migration connection must disable statement_timeout"
        );

        let lock_timeout = sqlx::query_scalar!(r#"SELECT current_setting('lock_timeout') AS "v!""#)
            .fetch_one(&mut conn)
            .await?;
        assert!(
            lock_timeout == "91s" || lock_timeout == "91000ms",
            "lock_timeout reported as {lock_timeout:?} — expected 91s/91000ms"
        );

        conn.close().await?;
        db.cleanup().await?;
        Ok(())
    }
}
