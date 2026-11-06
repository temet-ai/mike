#[path = "../../src/stack/lease.rs"]
mod lease;
#[path = "../../src/stack/protocol.rs"]
mod protocol;

#[test]
fn same_parent_selected_files_have_distinct_managed_directories() -> anyhow::Result<()> {
    use mike::config::paths::InstallationPaths;
    let dir = tempfile::tempdir()?;
    let first = InstallationPaths::resolve(Some(&dir.path().join("first.toml")))?;
    let second = InstallationPaths::resolve(Some(&dir.path().join("second.toml")))?;
    assert_ne!(first.installation_id, second.installation_id);
    assert_ne!(first.state_dir, second.state_dir);
    assert_ne!(first.data_dir, second.data_dir);
    assert_ne!(first.log_dir, second.log_dir);
    assert!(!first.state_dir.exists());
    assert!(!second.state_dir.exists());
    Ok(())
}

#[tokio::test]
async fn forged_pid_files_never_authorize_signals_or_mutation() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut config: mike::config::ProxyConfig = toml::from_str(
        "persist_enabled=false\nproxy_listen_addr='127.0.0.1:1'\ninspect_listen_addr='127.0.0.1:1'\nspa_enabled=false",
    )?;
    config.config_path = dir.path().join("config.toml");
    let record = dir.path().join("mike-serve.pid");
    for value in [
        "0".to_owned(),
        "-1".to_owned(),
        "invalid".to_owned(),
        std::process::id().to_string(),
    ] {
        std::fs::write(&record, &value)?;
        assert!(mike::stack::running_process(&config).is_none());
        assert!(matches!(
            mike::stack::stop_stack(&config).await?,
            mike::stack::StopOutcome::NotRunning
        ));
        assert_eq!(std::fs::read_to_string(&record)?, value);
    }
    Ok(())
}

#[test]
fn direct_wrapper_keeps_session_capture_without_enabling_remote_control() {
    let wiring = mike::claude::ClaudeWiring {
        proxy_addr: "127.0.0.1:9988",
        sidecar: None,
        operator_args: &[],
    };
    let id = uuid::Uuid::nil();
    let invocation =
        mike::claude::build_claude_invocation(&wiring, &["--model".into(), "sonnet".into()], id);
    assert_eq!(
        invocation.env["ANTHROPIC_BASE_URL"],
        "http://127.0.0.1:9988"
    );
    assert_eq!(
        invocation.args,
        ["--session-id", &id.to_string(), "--model", "sonnet"]
    );
}

#[test]
fn sidecar_requires_a_separate_explicit_opt_in_even_with_configured_ca() -> anyhow::Result<()> {
    let config: mike::config::ProxyConfig = toml::from_str("launch_ca_cert='/existing/ca.pem'")?;
    assert_eq!(serde_json::to_value(config)?["launch_use_sidecar"], false);
    let config: mike::config::ProxyConfig =
        toml::from_str("launch_use_sidecar=true\nlaunch_ca_cert='/existing/ca.pem'")?;
    assert_eq!(serde_json::to_value(config)?["launch_use_sidecar"], true);
    Ok(())
}
