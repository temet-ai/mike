use super::*;

fn config() -> ProxyConfig {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.proxy_listen_addr = "localhost:9988".into();
    config.inspect_listen_addr = "127.0.0.1:9989".into();
    config.spa_bind = "127.0.0.1:9990".into();
    config.spa_enabled = true;
    config
}

#[test]
fn unchanged_verified_ports_need_no_probe() -> Result<()> {
    let owned = [
        "127.0.0.1:9988".parse()?,
        "127.0.0.1:9989".parse()?,
        "127.0.0.1:9990".parse()?,
    ];
    let reservations = reserve_with::<()>(&config(), Some(&owned), |_| {
        anyhow::bail!("unchanged owner port must not be probed")
    });
    assert!(reservations.is_ok());
    Ok(())
}

#[test]
fn foreign_new_port_fails_before_stop() -> Result<()> {
    let owned = ["127.0.0.1:9988".parse()?, "127.0.0.1:9989".parse()?];
    let stopped = std::cell::Cell::new(false);
    let attempt = || -> Result<()> {
        reserve_with::<()>(&config(), Some(&owned), |address| {
            assert_eq!(address, "127.0.0.1:9990".parse::<SocketAddr>()?);
            anyhow::bail!("foreign listener")
        })?;
        stopped.set(true);
        Ok(())
    };
    assert!(attempt().is_err());
    assert!(!stopped.get());
    Ok(())
}

#[test]
fn unverified_config_never_exempts_occupied_ports() -> Result<()> {
    let mut probes = Vec::new();
    let result = reserve_with::<()>(&config(), None, |address| {
        probes.push(address);
        anyhow::bail!("unknown listener ownership")
    });
    assert!(result.is_err());
    assert_eq!(probes, vec!["127.0.0.1:9988".parse::<SocketAddr>()?]);
    Ok(())
}

#[test]
fn ephemeral_ports_are_probed_and_disabled_spa_is_ignored() -> Result<()> {
    let mut config = config();
    config.proxy_listen_addr = "127.0.0.1:0".into();
    config.inspect_listen_addr = "127.0.0.1:0".into();
    config.spa_enabled = false;
    let old_bound = ["127.0.0.1:43111".parse()?, "127.0.0.1:43112".parse()?];
    let reservations = reserve_with(&config, Some(&old_bound), Ok)?;
    assert_eq!(reservations.len(), 2);
    assert!(reservations.iter().all(|address| address.port() == 0));
    Ok(())
}

#[test]
fn reservations_survive_until_caller_releases_them() -> Result<()> {
    struct Guard(std::rc::Rc<std::cell::Cell<usize>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let released = std::rc::Rc::new(std::cell::Cell::new(0));
    let reservations = reserve_with(&config(), None, |_| Ok(Guard(released.clone())))?;
    assert_eq!(released.get(), 0);
    drop(reservations);
    assert_eq!(released.get(), 3);
    Ok(())
}

#[test]
fn legacy_owner_is_preserved_with_an_actionable_upgrade_error() -> Result<()> {
    let status: RuntimeStatus = serde_json::from_value(serde_json::json!({
        "identity": {"instance_id": "test", "generation": uuid::Uuid::nil(),
            "pid": 42, "started_unix_secs": 1},
        "ready": true, "stopping": false, "matches_requested": false,
        "persist_enabled": false
    }))?;
    let error = reserve_replacement(&config(), Some(&status))
        .err()
        .context("legacy owner must be preserved")?;
    assert!(
        error
            .to_string()
            .contains("explicitly stop it and start it again")
    );
    Ok(())
}
