use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use anyhow::Result;

use super::ProxyConfig;

pub fn inspect_url(config: &ProxyConfig) -> Result<String> {
    http_url(
        &config.inspect_listen_addr,
        config.unsupported_allow_remote_listeners,
    )
}

pub(crate) fn http_url(raw: &str, allow_remote: bool) -> Result<String> {
    let mut address = parse(raw)?;
    anyhow::ensure!(
        address.ip().is_loopback() || allow_remote,
        "selected listener is not local; explicit unsupported remote-listener opt-in is required"
    );
    if address.ip().is_unspecified() {
        address.set_ip(match address.ip() {
            IpAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
            IpAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
        });
    }
    Ok(format!("http://{address}"))
}

fn parse(raw: &str) -> Result<SocketAddr> {
    let normalized = match raw.rsplit_once(':') {
        Some((host, port))
            if host.eq_ignore_ascii_case("localhost")
                && !port.is_empty()
                && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            format!("127.0.0.1:{port}")
        }
        _ => raw.to_owned(),
    };
    normalized.parse().map_err(|_| {
        anyhow::anyhow!(
            "selected listener must be an IP literal or localhost with an explicit valid port"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_local_urls_never_interpret_userinfo_paths_or_dns_names() -> Result<()> {
        let mut config = ProxyConfig::for_tests_in_memory();
        config.inspect_listen_addr = "localhost:1234".into();
        assert_eq!(inspect_url(&config)?, "http://127.0.0.1:1234");
        assert_eq!(http_url("localhost:1234", false)?, "http://127.0.0.1:1234");
        assert_eq!(http_url("LOCALHOST:1234", false)?, "http://127.0.0.1:1234");
        assert_eq!(http_url("[::1]:1234", false)?, "http://[::1]:1234");
        for raw in [
            "secret@127.0.0.1:1234",
            "127.0.0.1:1234/path",
            "evil.example:1234",
            "localhost:+1234",
            "localhost:65536",
        ] {
            let error = http_url(raw, false)
                .err()
                .ok_or_else(|| anyhow::anyhow!("unsafe listener was accepted"))?;
            assert!(!error.to_string().contains("secret"));
        }
        for raw in ["0.0.0.0:1234", "[::]:1234", "192.168.0.1:1234"] {
            assert!(http_url(raw, false).is_err());
        }
        assert_eq!(http_url("0.0.0.0:1234", true)?, "http://127.0.0.1:1234");
        assert_eq!(http_url("[::]:1234", true)?, "http://[::1]:1234");
        Ok(())
    }
}
