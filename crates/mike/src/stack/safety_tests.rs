use super::*;

#[tokio::test]
async fn forged_legacy_pid_records_never_supply_stop_authority() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut config = crate::config::ProxyConfig::for_tests_in_memory();
    config.config_path = dir.path().join("config.toml");
    config.log_dir = Some(dir.path().join("logs"));
    config.proxy_listen_addr = "127.0.0.1:1".into();
    config.inspect_listen_addr = "127.0.0.1:1".into();
    config.spa_enabled = false;
    let pid_file = dir.path().join("mike-serve.pid");
    for text in [
        "0",
        "-1",
        "garbage",
        "4294967295",
        &std::process::id().to_string(),
    ] {
        std::fs::write(&pid_file, text)?;
        assert!(running_process(&config).is_none());
        assert!(matches!(
            stop_stack(&config).await?,
            StopOutcome::NotRunning
        ));
        assert_eq!(std::fs::read_to_string(&pid_file)?, text);
    }
    Ok(())
}

#[test]
fn smoke_run_boots_beside_the_owner_and_leaves_its_record_alone() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut config = crate::config::ProxyConfig::for_tests_in_memory();
    config.config_path = dir.path().join("config.toml");
    let live = DaemonOwner::acquire(&config)?;
    let record_before = std::fs::read_to_string(live.spec.owner_file())?;
    assert!(DaemonOwner::acquire(&config).is_err());

    config.smoke_run = true;
    let smoke = DaemonOwner::acquire_unless_smoke_run(&config)?;
    assert!(!smoke.owns_installation);
    assert_eq!(smoke.identity.instance_id, live.identity.instance_id);
    assert_ne!(smoke.identity.generation, live.identity.generation);
    drop(smoke);
    assert_eq!(
        std::fs::read_to_string(live.spec.owner_file())?,
        record_before
    );

    config.smoke_run = false;
    assert!(DaemonOwner::acquire_unless_smoke_run(&config).is_err());
    Ok(())
}

#[test]
fn invalid_private_identity_does_not_get_overwritten() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut config = crate::config::ProxyConfig::for_tests_in_memory();
    config.config_path = dir.path().join("config.toml");
    let spec = instance::InstanceSpec::from_config(&config)?;
    crate::config::write::ensure_private_directory(&spec.paths.state_dir)?;
    std::fs::write(spec.owner_file(), "unrelated-private-file")?;
    assert!(DaemonOwner::acquire(&config).is_err());
    assert_eq!(
        std::fs::read_to_string(spec.owner_file())?,
        "unrelated-private-file"
    );
    Ok(())
}
