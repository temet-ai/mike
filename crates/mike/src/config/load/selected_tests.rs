use super::*;
use crate::config::test_env::*;

#[test]
fn selected_toml_env_cli_and_reload_preserve_precedence() -> anyhow::Result<()> {
    let _serial = env_lock().lock().map_err(|_| anyhow::anyhow!("env lock"))?;
    let _cleared = clear_proxy_env();
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "max_requests=123\nproxy_listen_addr='127.0.0.1:7000'\n",
    )?;
    let _env = EnvVarGuard::set("MIKE_LISTEN_ADDR", "127.0.0.1:7001");
    let cli = CliOverrides {
        config: Some(path.clone()),
        max_requests: Some(789),
        ..CliOverrides::default()
    };
    let config = load(cli)?;
    assert_eq!(config.proxy_listen_addr, "127.0.0.1:7001");
    assert_eq!(config.max_requests, 789);
    std::fs::write(
        &path,
        "max_requests=456\nproxy_listen_addr='127.0.0.1:7002'\n",
    )?;
    let reloaded = reload_selected(&config)?;
    assert_eq!(reloaded.max_requests, 789);
    assert_eq!(reloaded.proxy_listen_addr, "127.0.0.1:7001");
    Ok(())
}

#[test]
fn config_debug_and_parse_errors_never_echo_credential_values() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    let config = ProxyConfig {
        database_url: Some("planted-secret".into()),
        ..ProxyConfig::default()
    };
    assert!(!format!("{config:?}").contains("planted-secret"));
    let cli = CliOverrides {
        database_url: Some("planted-secret".into()),
        ..CliOverrides::default()
    };
    assert!(!format!("{cli:?}").contains("planted-secret"));
    std::fs::write(&path, "max_requests='planted-secret'")?;
    let error = load(CliOverrides {
        config: Some(path),
        ..CliOverrides::default()
    })
    .err()
    .ok_or_else(|| anyhow::anyhow!("bad type was accepted"))?;
    assert!(!format!("{error:#}").contains("planted-secret"));
    Ok(())
}

#[test]
fn selected_relative_data_paths_follow_config_instead_of_cwd() -> anyhow::Result<()> {
    let _serial = env_lock().lock().map_err(|_| anyhow::anyhow!("env lock"))?;
    let _cleared = clear_proxy_env();
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "data_dir='captures'\nlog_dir='private logs'\n")?;
    let config = load(CliOverrides {
        config: Some(path),
        ..CliOverrides::default()
    })?;
    assert_eq!(config.data_dir, Some(dir.path().join("captures")));
    assert_eq!(config.log_dir, Some(dir.path().join("private logs")));
    assert!(!dir.path().join("captures").exists());
    Ok(())
}
