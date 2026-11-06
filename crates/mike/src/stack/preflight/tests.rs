use std::cell::RefCell;
use std::future::ready;

use super::{DatabaseFailure, after_preflight, requested_boot_with};
use crate::config::ProxyConfig;

fn persistent_config() -> super::RequestedBoot {
    super::RequestedBoot {
        persist_enabled: true,
        database_url: Some("primary".to_owned()),
        local_database_url: Some("fallback".to_owned()),
        db_fallback_enabled: true,
        listeners: ProxyConfig::for_tests_in_memory(),
    }
}

#[tokio::test]
async fn database_rejection_never_stops_or_spawns() {
    for reason in ["bad password", "incompatible ledger", "missing extension"] {
        let mut config = persistent_config();
        config.db_fallback_enabled = false;
        let actions = RefCell::new(Vec::new());
        let preflight = requested_boot_with(
            &config,
            |_| ready(false),
            |_| {
                let error = anyhow::anyhow!(reason);
                ready(Err(if reason == "bad password" {
                    DatabaseFailure::Connection(error)
                } else {
                    DatabaseFailure::Invalid(error)
                }))
            },
        );
        let result = after_preflight(preflight, || async {
            actions.borrow_mut().extend(["stop", "spawn"]);
            Ok(())
        })
        .await;
        assert!(result.is_err(), "{reason}");
        assert!(actions.borrow().is_empty(), "{reason}");
    }
}

#[tokio::test]
async fn listener_rejection_never_validates_database_stops_or_spawns() {
    for address in ["0.0.0.0:9988", "localhost:65536", "invalid"] {
        for listener in ["proxy", "inspect", "spa"] {
            let mut config = persistent_config();
            match listener {
                "proxy" => config.listeners.proxy_listen_addr = address.to_owned(),
                "inspect" => config.listeners.inspect_listen_addr = address.to_owned(),
                _ => config.listeners.spa_bind = address.to_owned(),
            }
            let actions = RefCell::new(Vec::new());
            let preflight = requested_boot_with(
                &config,
                |_| ready(false),
                |_| {
                    actions.borrow_mut().push("database");
                    ready(Ok(()))
                },
            );
            let result = after_preflight(preflight, || async {
                actions.borrow_mut().extend(["stop", "spawn"]);
                Ok(())
            })
            .await;
            assert!(result.is_err(), "{listener}: {address}");
            assert!(actions.borrow().is_empty());
        }
    }
}

#[tokio::test]
async fn incompatible_primary_never_uses_fallback() {
    let config = persistent_config();
    let checked = RefCell::new(Vec::new());
    let result = requested_boot_with(
        &config,
        |_| ready(false),
        |url| {
            checked.borrow_mut().push(url);
            ready(Err(DatabaseFailure::Invalid(anyhow::anyhow!(
                "incompatible ledger"
            ))))
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(*checked.borrow(), ["primary"]);
}

#[tokio::test]
async fn connection_failure_uses_validated_fallback() -> anyhow::Result<()> {
    let config = persistent_config();
    let checked = RefCell::new(Vec::new());
    requested_boot_with(
        &config,
        |_| ready(false),
        |url| {
            let primary = url == "primary";
            checked.borrow_mut().push(url);
            ready(if primary {
                Err(DatabaseFailure::Connection(anyhow::anyhow!("connection")))
            } else {
                Ok(())
            })
        },
    )
    .await?;
    assert_eq!(*checked.borrow(), ["primary", "fallback"]);
    Ok(())
}

#[tokio::test]
async fn unreachable_primary_validates_only_fallback() -> anyhow::Result<()> {
    let config = persistent_config();
    let checked = RefCell::new(Vec::new());
    requested_boot_with(
        &config,
        |_| ready(true),
        |url| {
            checked.borrow_mut().push(url);
            ready(Ok(()))
        },
    )
    .await?;
    assert_eq!(*checked.borrow(), ["fallback"]);
    Ok(())
}

#[tokio::test]
async fn invalid_fallback_never_stops_or_spawns() {
    let config = persistent_config();
    let actions = RefCell::new(Vec::new());
    let preflight = requested_boot_with(
        &config,
        |_| ready(true),
        |_| ready(Err(DatabaseFailure::Invalid(anyhow::anyhow!("extension")))),
    );
    let result = after_preflight(preflight, || async {
        actions.borrow_mut().extend(["stop", "spawn"]);
        Ok(())
    })
    .await;
    assert!(result.is_err());
    assert!(actions.borrow().is_empty());
}

#[tokio::test]
async fn trial_without_database_proceeds_without_database_work() -> anyhow::Result<()> {
    let mut config = persistent_config();
    config.persist_enabled = false;
    config.database_url = None;
    config.listeners.spa_enabled = false;
    config.listeners.spa_bind = "unused invalid address".to_owned();
    let actions = RefCell::new(Vec::new());
    let preflight = requested_boot_with(
        &config,
        |_| {
            actions.borrow_mut().push("probe");
            ready(false)
        },
        |_| {
            actions.borrow_mut().push("database");
            ready(Ok(()))
        },
    );
    after_preflight(preflight, || async {
        actions.borrow_mut().extend(["stop", "spawn"]);
        Ok(())
    })
    .await?;
    assert_eq!(*actions.borrow(), ["stop", "spawn"]);
    Ok(())
}

#[tokio::test]
async fn missing_persistent_target_never_stops_or_spawns() {
    let mut config = persistent_config();
    config.database_url = None;
    let actions = RefCell::new(Vec::new());
    let preflight = requested_boot_with(&config, |_| ready(false), |_| ready(Ok(())));
    let result = after_preflight(preflight, || async {
        actions.borrow_mut().extend(["stop", "spawn"]);
        Ok(())
    })
    .await;
    assert!(result.is_err());
    assert!(actions.borrow().is_empty());
}

#[tokio::test]
async fn ready_primary_does_not_validate_unused_fallback() -> anyhow::Result<()> {
    let config = persistent_config();
    let checked = RefCell::new(Vec::new());
    requested_boot_with(
        &config,
        |_| ready(false),
        |url| {
            checked.borrow_mut().push(url);
            ready(Ok(()))
        },
    )
    .await?;
    assert_eq!(*checked.borrow(), ["primary"]);
    Ok(())
}

#[tokio::test]
async fn duplicate_listeners_never_stop_or_spawn() {
    let mut config = persistent_config();
    config.listeners.proxy_listen_addr = "localhost:9988".to_owned();
    config.listeners.inspect_listen_addr = "127.0.0.1:9988".to_owned();
    let actions = RefCell::new(Vec::new());
    let preflight = requested_boot_with(&config, |_| ready(false), |_| ready(Ok(())));
    let result = after_preflight(preflight, || async {
        actions.borrow_mut().extend(["stop", "spawn"]);
        Ok(())
    })
    .await;
    assert!(result.is_err());
    assert!(actions.borrow().is_empty());
}

#[tokio::test]
async fn stalled_cleanup_is_bounded_and_never_stops_or_spawns() {
    let actions = RefCell::new(Vec::new());
    let validation = super::bounded_validation(std::time::Duration::from_millis(1), async {
        actions.borrow_mut().push("assessed");
        std::future::pending::<()>().await;
        Ok(())
    });
    let result = after_preflight(
        async { validation.await.map_err(DatabaseFailure::into_error) },
        || async {
            actions.borrow_mut().extend(["stop", "spawn"]);
            Ok(())
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(*actions.borrow(), ["assessed"]);
}

#[tokio::test]
async fn current_ledger_without_schema_never_stops_or_spawns() {
    let actions = RefCell::new(Vec::new());
    let preflight = super::verify_current_schema(0, || ready(Ok(false)));
    let result = after_preflight(preflight, || async {
        actions.borrow_mut().extend(["stop", "spawn"]);
        Ok(())
    })
    .await;
    assert!(result.is_err());
    assert!(actions.borrow().is_empty());
}

#[tokio::test]
async fn pending_migrations_do_not_require_schema_already_present() -> anyhow::Result<()> {
    let checked = RefCell::new(false);
    super::verify_current_schema(1, || {
        *checked.borrow_mut() = true;
        ready(Ok(false))
    })
    .await?;
    assert!(!*checked.borrow());
    super::verify_current_schema(0, || ready(Ok(true))).await
}
