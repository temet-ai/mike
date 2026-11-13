use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

use super::{PoolClass, PoolClassLimits, PoolClasses, PoolClassesConfig};
use crate::store::persistence::test_support as test_util;
use crate::store::persistence::{PersistError, PersistHandle, SchemaPolicy};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

fn narrow_classes() -> PoolClassesConfig {
    PoolClassesConfig {
        capture: PoolClassLimits {
            max_connections: 2,
            acquire_timeout_ms: 2_000,
            statement_timeout_ms: 4_000,
        },
        interactive: PoolClassLimits {
            max_connections: 3,
            acquire_timeout_ms: 1_500,
            statement_timeout_ms: 4_500,
        },
        background: PoolClassLimits {
            max_connections: 1,
            acquire_timeout_ms: 300,
            statement_timeout_ms: 6_000,
        },
    }
}

async fn statement_timeout(handle: &PersistHandle) -> anyhow::Result<String> {
    let (value,): (String,) = sqlx::query_as("SHOW statement_timeout")
        .fetch_one(handle.pool())
        .await?;
    Ok(value)
}

#[tokio::test]
async fn connect_classed_three_classes_distinct_pools_with_configured_limits() -> anyhow::Result<()>
{
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let classes = narrow_classes();
    let handle = PersistHandle::connect_classed(
        &db.database_url,
        CONNECT_TIMEOUT,
        &classes,
        SchemaPolicy::Migrate,
    )
    .await?;

    let labels: Vec<&str> = handle
        .class_pools()
        .into_iter()
        .map(|(label, _)| label)
        .collect();
    assert_eq!(labels, vec!["capture", "interactive", "background"]);

    for (class, limits, expected_timeout) in [
        (PoolClass::Capture, classes.capture, "4s"),
        (PoolClass::Interactive, classes.interactive, "4500ms"),
        (PoolClass::Background, classes.background, "6s"),
    ] {
        let classed = handle.on(class);
        let options = classed.pool().options();
        assert_eq!(
            options.get_max_connections(),
            limits.max_connections,
            "{class:?} max_connections"
        );
        assert_eq!(
            options.get_acquire_timeout(),
            Duration::from_millis(limits.acquire_timeout_ms),
            "{class:?} acquire_timeout"
        );
        assert_eq!(
            statement_timeout(&classed).await?,
            expected_timeout,
            "{class:?} statement_timeout"
        );
    }

    for (_, pool) in handle.class_pools() {
        pool.close().await;
    }
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn connect_classed_saturated_background_capture_still_acquires() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect_classed(
        &db.database_url,
        CONNECT_TIMEOUT,
        &narrow_classes(),
        SchemaPolicy::Migrate,
    )
    .await?;

    let background = handle.background();
    let held = background.pool().acquire().await?;
    let starved = background.pool().acquire().await.err();
    assert!(
        matches!(starved, Some(sqlx::Error::PoolTimedOut)),
        "a second background acquire must time out while the only connection is held, got {starved:?}"
    );

    let capture = handle.capture().pool().acquire().await?;
    let interactive = handle.on(PoolClass::Interactive).pool().acquire().await?;
    assert_eq!(handle.background_pool().size(), 1);

    drop(capture);
    drop(interactive);
    drop(held);
    for (_, pool) in handle.class_pools() {
        pool.close().await;
    }
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn on_pool_class_shares_in_process_state() -> anyhow::Result<()> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost/pool_classes_fixture")?;
    let handle = PersistHandle::from_pools(PoolClasses::single(pool));
    let background = handle.background();
    assert!(Arc::ptr_eq(
        &handle.main_lane_captured,
        &background.main_lane_captured
    ));
    assert_eq!(handle.class_pools().len(), 1);
    assert_eq!(handle.class_pools()[0].0, "shared");
    Ok(())
}

#[tokio::test]
async fn connect_classed_unreachable_host_reports_redacted_connect_error() -> anyhow::Result<()> {
    let url = "postgres://proxy_test:proxy_test@127.0.0.1:1/claude_proxy_never";
    let result = PersistHandle::connect_classed(
        url,
        Duration::from_millis(500),
        &PoolClassesConfig::default(),
        SchemaPolicy::Migrate,
    )
    .await;
    match result {
        Err(PersistError::Connect { url: redacted, .. }) => {
            assert!(!redacted.contains("proxy_test:proxy_test"));
            assert!(redacted.contains("127.0.0.1:1"));
            Ok(())
        }
        Err(other) => anyhow::bail!("expected Connect, got {other:?}"),
        Ok(_) => anyhow::bail!("expected error, got handle"),
    }
}
