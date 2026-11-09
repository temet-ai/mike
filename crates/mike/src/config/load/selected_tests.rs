use super::*;
use crate::environment::MapEnvironment;

#[test]
fn selected_toml_env_cli_and_reload_preserve_precedence() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated().with("MIKE_LISTEN_ADDR", "127.0.0.1:7001");
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "max_requests=123\nproxy_listen_addr='127.0.0.1:7000'\n",
    )?;
    let cli = CliOverrides {
        config: Some(path.clone()),
        max_requests: Some(789),
        ..CliOverrides::default()
    };
    let config = load_with(cli, &environment)?;
    assert_eq!(config.proxy_listen_addr, "127.0.0.1:7001");
    assert_eq!(config.max_requests, 789);
    std::fs::write(
        &path,
        "max_requests=456\nproxy_listen_addr='127.0.0.1:7002'\n",
    )?;
    let reloaded = reload_selected_with(&config, &environment)?;
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
    let error = load_with(
        CliOverrides {
            config: Some(path),
            ..CliOverrides::default()
        },
        &MapEnvironment::isolated(),
    )
    .err()
    .ok_or_else(|| anyhow::anyhow!("bad type was accepted"))?;
    assert!(!format!("{error:#}").contains("planted-secret"));
    Ok(())
}

#[test]
fn selected_relative_data_paths_follow_config_instead_of_cwd() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "data_dir='captures'\nlog_dir='private logs'\n")?;
    let config = load_with(
        CliOverrides {
            config: Some(path),
            ..CliOverrides::default()
        },
        &MapEnvironment::isolated(),
    )?;
    assert_eq!(config.data_dir, Some(dir.path().join("captures")));
    assert_eq!(config.log_dir, Some(dir.path().join("private logs")));
    assert!(!dir.path().join("captures").exists());
    Ok(())
}

#[test]
fn selected_file_reference_to_a_named_variable_reads_the_injected_environment() -> anyhow::Result<()>
{
    use secrecy::ExposeSecret;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "steerer_openrouter_api_key_from_env='MY_KEY'\n")?;
    let environment = MapEnvironment::isolated().with("MY_KEY", "sk-from-map");
    let config = load_with(
        CliOverrides {
            config: Some(path),
            ..CliOverrides::default()
        },
        &environment,
    )?;
    assert_eq!(
        config
            .steerer
            .openrouter_api_key
            .as_ref()
            .map(|key| key.expose_secret().to_owned()),
        Some("sk-from-map".to_owned())
    );
    Ok(())
}
