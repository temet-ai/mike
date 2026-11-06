use super::DbPoolConfig;
use crate::config::test_env::{EnvVarGuard, clear_proxy_env, env_lock};
use crate::config::{CliOverrides, ProxyConfig, load};
use crate::store::persistence::pool_classes::PoolClassesConfig;

#[test]
fn db_pool_defaults_match_the_work_class_design() {
    let classes = DbPoolConfig::default().classes();
    assert_eq!(classes, PoolClassesConfig::default());
    assert_eq!(classes.capture.max_connections, 4);
    assert_eq!(classes.capture.acquire_timeout_ms, 5_000);
    assert_eq!(classes.capture.statement_timeout_ms, 5_000);
    assert_eq!(classes.interactive.max_connections, 6);
    assert_eq!(classes.interactive.acquire_timeout_ms, 3_000);
    assert_eq!(classes.interactive.statement_timeout_ms, 15_000);
    assert_eq!(classes.background.max_connections, 4);
    assert_eq!(classes.background.acquire_timeout_ms, 30_000);
    assert_eq!(classes.background.statement_timeout_ms, 30_000);
}

#[test]
fn db_pool_env_overrides_each_class_limit() -> anyhow::Result<()> {
    let _serial = env_lock()
        .lock()
        .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
    let _cleared = clear_proxy_env();
    let _capture = EnvVarGuard::set("MIKE_DB_POOL_CAPTURE_SIZE", "3");
    let _interactive = EnvVarGuard::set("MIKE_DB_POOL_INTERACTIVE_ACQUIRE_TIMEOUT_MS", "750");
    let _background = EnvVarGuard::set("MIKE_DB_POOL_BACKGROUND_STATEMENT_TIMEOUT_MS", "90000");

    let config = load(CliOverrides::default())?;
    let classes = config.db_pool.classes();
    assert_eq!(classes.capture.max_connections, 3);
    assert_eq!(classes.interactive.acquire_timeout_ms, 750);
    assert_eq!(classes.background.statement_timeout_ms, 90_000);
    assert_eq!(classes.interactive.max_connections, 6);
    Ok(())
}

#[test]
fn db_pool_fields_serialize_as_flat_boot_keys() -> anyhow::Result<()> {
    let value = serde_json::to_value(ProxyConfig::default())?;
    for key in [
        "db_pool_capture_size",
        "db_pool_capture_acquire_timeout_ms",
        "db_pool_capture_statement_timeout_ms",
        "db_pool_interactive_size",
        "db_pool_interactive_acquire_timeout_ms",
        "db_pool_interactive_statement_timeout_ms",
        "db_pool_background_size",
        "db_pool_background_acquire_timeout_ms",
        "db_pool_background_statement_timeout_ms",
    ] {
        assert!(value.get(key).is_some(), "{key} missing from boot config");
    }
    Ok(())
}

#[test]
fn db_pool_non_numeric_env_is_rejected() -> anyhow::Result<()> {
    let _serial = env_lock()
        .lock()
        .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
    let _cleared = clear_proxy_env();
    let _capture = EnvVarGuard::set("MIKE_DB_POOL_CAPTURE_SIZE", "four");
    assert!(load(CliOverrides::default()).is_err());
    Ok(())
}
