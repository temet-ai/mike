use std::time::Duration;

use crate::config::ProxyConfig;
use crate::setup::database;
use crate::store::persistence::pool;

mod policy;

pub(super) use policy::after_preflight;

pub(super) async fn requested_boot(config: &ProxyConfig) -> anyhow::Result<()> {
    let acquire_timeout = Duration::from_millis(config.db_connect_timeout_ms);
    let requested = policy::RequestedBoot {
        listeners: config.clone(),
        persist_enabled: config.persist_enabled,
        database_url: config.database_url.clone(),
        local_database_url: config.local_database_url.clone(),
        db_fallback_enabled: config.db_fallback_enabled,
    };
    policy::requested_boot_with(
        &requested,
        |url| async move { crate::config::primary_tcp_unreachable(&url).await },
        |url| async move { validate_database(&url, acquire_timeout).await },
    )
    .await
}

async fn validate_database(
    url: &str,
    acquire_timeout: Duration,
) -> Result<(), policy::DatabaseFailure> {
    policy::bounded_validation(
        acquire_timeout.saturating_add(Duration::from_secs(20)),
        validate_and_close(url, acquire_timeout),
    )
    .await
}

async fn validate_and_close(
    url: &str,
    acquire_timeout: Duration,
) -> Result<(), policy::DatabaseFailure> {
    database::validate_url(url).map_err(policy::DatabaseFailure::Invalid)?;
    let connection = pool::build_read_only_pool_with_timeout(url, acquire_timeout)
        .await
        .map_err(|_| connection_failure())?;
    let result = tokio::time::timeout(Duration::from_secs(10), assess_boot_database(&connection))
        .await
        .map_err(|_| anyhow::anyhow!("database readiness assessment exceeded ten seconds"))
        .and_then(|result| result);
    connection.close().await;
    result.map_err(policy::DatabaseFailure::Invalid)
}

fn connection_failure() -> policy::DatabaseFailure {
    policy::DatabaseFailure::Connection(anyhow::anyhow!(
        "database authentication or connection failed; check the host, database, user and credential"
    ))
}

async fn assess_boot_database(connection: &sqlx::PgPool) -> anyhow::Result<()> {
    let readiness = database::assess(connection).await?;
    policy::verify_current_schema(readiness.pending_migrations, || async {
        crate::store::persistence::startup_health::schema_exists(connection)
            .await
            .map_err(|_| {
                anyhow::anyhow!("database schema assessment failed; check schema visibility")
            })
    })
    .await
}
