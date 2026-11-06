use clap::Parser;

use crate::config::load::CliOverrides;

use super::schema::ProxyConfig;

fn parse(args: &[&str]) -> anyhow::Result<ProxyConfig> {
    let overrides = CliOverrides::try_parse_from(args)?;
    crate::config::load::load(overrides)
}

#[test]
fn test_default_config_parses() -> anyhow::Result<()> {
    let _serial = crate::config::test_env::env_lock()
        .lock()
        .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
    let _cleared = crate::config::test_env::clear_proxy_env();
    let config = parse(&["mike"])?;
    assert_eq!(config.proxy_listen_addr, "127.0.0.1:9988");
    assert_eq!(config.inspect_listen_addr, "127.0.0.1:9989");
    assert_eq!(config.upstream_url, "https://api.anthropic.com");
    assert_eq!(config.max_requests, 1000);
    assert_eq!(config.max_age, "24h");
    assert!(config.otel_endpoint.is_none());
    assert_eq!(config.log_format, "pretty");
    assert_eq!(config.log_level, "info");
    assert!(
        config.log_dir.is_some(),
        "loaded configuration resolves private per-user logs"
    );
    assert_eq!(config.log_max_files, 7);
    assert!(
        config.persist_enabled,
        "persist_enabled defaults to true; operators opt out with persist_enabled = false"
    );
    assert!(
        config.database_url.is_none(),
        "database_url has no default — operators must set MIKE_DATABASE_URL"
    );
    assert!(
        config.data_dir.is_some(),
        "loaded configuration resolves the per-user data path"
    );
    Ok(())
}

#[test]
fn test_custom_listen_addr() -> anyhow::Result<()> {
    let config = parse(&["mike", "--proxy-listen-addr", "0.0.0.0:9999"])?;
    assert_eq!(config.proxy_listen_addr, "0.0.0.0:9999");
    assert_eq!(config.inspect_listen_addr, "127.0.0.1:9989");
    Ok(())
}

#[test]
fn test_custom_inspect_addr() -> anyhow::Result<()> {
    let config = parse(&["mike", "--inspect-listen-addr", "0.0.0.0:8899"])?;
    assert_eq!(config.inspect_listen_addr, "0.0.0.0:8899");
    assert_eq!(config.proxy_listen_addr, "127.0.0.1:9988");
    Ok(())
}
