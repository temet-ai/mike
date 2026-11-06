use std::net::SocketAddr;

use anyhow::{Context, bail};

use crate::config::ProxyConfig;

pub(crate) fn validate(config: &ProxyConfig) -> anyhow::Result<()> {
    let mut bound = std::collections::HashSet::new();
    for (name, address, enabled) in [
        ("proxy", config.proxy_listen_addr.as_str(), true),
        ("inspect", config.inspect_listen_addr.as_str(), true),
        ("SPA", config.spa_bind.as_str(), config.spa_enabled),
    ] {
        if enabled {
            let normalized = bind_address(address, config.unsupported_allow_remote_listeners)
                .with_context(|| format!("invalid {name} listener"))?;
            let socket: SocketAddr = normalized.parse()?;
            anyhow::ensure!(
                socket.port() == 0 || bound.insert(socket),
                "duplicate {name} listener address {normalized}; enabled listeners require distinct ports or addresses"
            );
        }
    }
    if config.unsupported_allow_remote_listeners {
        tracing::warn!(
            "remote listeners explicitly enabled: outside the supported local-only preview; local API guards do not authenticate remote clients"
        );
    }
    Ok(())
}

pub(crate) fn bind_address(raw: &str, allow_remote: bool) -> anyhow::Result<String> {
    if let Ok(addr) = raw.parse::<SocketAddr>() {
        if allow_remote || addr.ip().is_loopback() {
            return Ok(addr.to_string());
        }
        bail!(
            "listener {raw} is not loopback; remote exposure requires MIKE_UNSUPPORTED_ALLOW_REMOTE_LISTENERS=1 and is outside the supported preview"
        );
    }
    if let Some((host, port)) = raw.rsplit_once(':')
        && host.eq_ignore_ascii_case("localhost")
        && !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
    {
        let port: u16 = port.parse().context("invalid localhost listener port")?;
        return Ok(format!("127.0.0.1:{port}"));
    }
    bail!("listener {raw} must be an IP literal or localhost with an explicit port");
}

#[cfg(test)]
mod tests {
    use super::{bind_address, validate};
    use crate::config::ProxyConfig;

    #[test]
    fn default_config_is_local_only() -> anyhow::Result<()> {
        let config = ProxyConfig::for_tests_in_memory();
        assert!(!config.unsupported_allow_remote_listeners);
        validate(&config)
    }

    #[test]
    fn local_bind_addresses_preserve_ports() -> anyhow::Result<()> {
        for raw in ["127.0.0.1:0", "127.0.0.2:18080", "[::1]:9990"] {
            assert_eq!(bind_address(raw, false)?, raw);
        }
        assert_eq!(bind_address("localhost:80", false)?, "127.0.0.1:80");
        Ok(())
    }

    #[test]
    fn remote_bind_addresses_require_explicit_escape() -> anyhow::Result<()> {
        for raw in ["0.0.0.0:9989", "[::]:9990", "192.168.1.20:9988"] {
            assert!(bind_address(raw, false).is_err(), "{raw}");
            assert_eq!(bind_address(raw, true)?, raw);
        }
        Ok(())
    }

    #[test]
    fn malformed_bind_addresses_are_always_rejected() {
        for raw in [
            "localhost",
            "localhost:",
            "localhost:65536",
            "localhost.evil:9989",
            "[::1]",
            "127.1:9989",
        ] {
            assert!(bind_address(raw, false).is_err(), "{raw}");
            assert!(bind_address(raw, true).is_err(), "{raw}");
        }
    }

    #[test]
    fn normalized_duplicate_enabled_listeners_are_rejected() {
        let mut config = ProxyConfig::for_tests_in_memory();
        config.proxy_listen_addr = "localhost:9988".to_owned();
        config.inspect_listen_addr = "127.0.0.1:9988".to_owned();
        assert!(validate(&config).is_err());
        config.inspect_listen_addr = "127.0.0.1:9989".to_owned();
        config.spa_bind = "127.0.0.1:9988".to_owned();
        assert!(validate(&config).is_err());
        config.spa_enabled = false;
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn independent_ephemeral_ports_are_allowed() -> anyhow::Result<()> {
        let mut config = ProxyConfig::for_tests_in_memory();
        config.proxy_listen_addr = "127.0.0.1:0".to_owned();
        config.inspect_listen_addr = "127.0.0.1:0".to_owned();
        config.spa_bind = "127.0.0.1:0".to_owned();
        validate(&config)
    }

    #[test]
    fn every_enabled_listener_is_validated() {
        for listener in ["proxy", "inspect", "spa"] {
            let mut config = ProxyConfig::for_tests_in_memory();
            let address = match listener {
                "proxy" => &mut config.proxy_listen_addr,
                "inspect" => &mut config.inspect_listen_addr,
                _ => &mut config.spa_bind,
            };
            *address = "0.0.0.0:9989".to_owned();
            assert!(validate(&config).is_err(), "{listener}");
        }
    }
}
