use std::net::IpAddr;

use serde::Serialize;

use crate::analysis::embeddings::EmbeddingConfig;
use crate::analysis::rerank::RerankConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressState {
    Loopback,
    RemoteBlocked,
    RemoteAllowed,
}

pub fn url_host(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    parsed
        .host_str()
        .map(|h| h.trim_matches(['[', ']']).to_ascii_lowercase())
}

pub fn is_loopback_url(url: &str) -> bool {
    let Some(host) = url_host(url) else {
        return false;
    };
    if host == "localhost" {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

pub fn egress_state_for_url(url: &str, allow_remote: bool) -> EgressState {
    if is_loopback_url(url) {
        EgressState::Loopback
    } else if allow_remote {
        EgressState::RemoteAllowed
    } else {
        EgressState::RemoteBlocked
    }
}

pub fn egress_state(config: &EmbeddingConfig, allow_remote: bool) -> EgressState {
    egress_state_for_url(&config.url, allow_remote)
}

pub fn allow_rerank(config: &RerankConfig, allow_remote: bool) -> bool {
    egress_state_for_url(&config.url, allow_remote) != EgressState::RemoteBlocked
}

pub fn allow(config: &EmbeddingConfig, allow_remote: bool) -> bool {
    egress_state(config, allow_remote) != EgressState::RemoteBlocked
}
