use axum::http::HeaderValue;

use crate::config::ProxyConfig;

mod authority;
mod middleware;
mod policy;

pub(crate) use middleware::guard;
pub(crate) use policy::LocalAccessPolicy;

pub(crate) fn effective_origins(configured: &[String]) -> Vec<HeaderValue> {
    policy::configured_origins(configured)
        .filter_map(authority::Origin::parse)
        .filter_map(|origin| origin.serialize().parse().ok())
        .collect()
}

pub(crate) fn origin_is_allowed(origin: &str, host: Option<&str>, configured: &[String]) -> bool {
    policy::origin_is_allowed(origin, host, configured)
}

impl LocalAccessPolicy {
    pub(crate) fn proxy(config: &ProxyConfig) -> Self {
        Self::for_proxy(&config.inspect_allowed_hosts, &config.spa_origins)
    }

    fn for_proxy(allowed_hosts: &[String], origins: &[String]) -> Self {
        let mut policy = Self::new("127.0.0.1:80", allowed_hosts, origins);
        policy.any_port = true;
        policy.authorities.extend(
            ["api.anthropic.com"]
                .into_iter()
                .filter_map(|raw| authority::Authority::parse(raw, 80)),
        );
        policy
    }

    pub(crate) fn inspect(config: &ProxyConfig) -> Self {
        Self::new(
            &config.inspect_listen_addr,
            &config.inspect_allowed_hosts,
            &config.spa_origins,
        )
    }

    pub(crate) fn inspect_on(config: &ProxyConfig, addr: std::net::SocketAddr) -> Self {
        Self::new(
            &addr.to_string(),
            &config.inspect_allowed_hosts,
            &config.spa_origins,
        )
    }

    pub(crate) fn spa_on(config: &ProxyConfig, addr: std::net::SocketAddr) -> Self {
        Self::new(
            &addr.to_string(),
            &config.inspect_allowed_hosts,
            &config.spa_origins,
        )
    }

    pub(crate) fn spa(config: &ProxyConfig) -> Self {
        Self::new(
            &config.spa_bind,
            &config.inspect_allowed_hosts,
            &config.spa_origins,
        )
    }
}
