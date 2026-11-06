use std::net::{SocketAddr, TcpListener};

use anyhow::{Context, Result};

use super::protocol::RuntimeStatus;
use crate::config::ProxyConfig;
use crate::serve::listener_policy;

pub(super) type Reservations = Vec<TcpListener>;

pub(super) fn reserve(config: &ProxyConfig, owned: Option<&[SocketAddr]>) -> Result<Reservations> {
    reserve_with(config, owned, |address| {
        TcpListener::bind(address).with_context(|| {
            format!(
                "requested listener {address} cannot be reserved without verified current ownership; existing daemon was preserved"
            )
        })
    })
}

fn reserve_with<T>(
    config: &ProxyConfig,
    owned: Option<&[SocketAddr]>,
    mut bind: impl FnMut(SocketAddr) -> Result<T>,
) -> Result<Vec<T>> {
    listener_policy::validate(config)?;
    let mut reservations = Vec::new();
    for raw in [
        Some(config.proxy_listen_addr.as_str()),
        Some(config.inspect_listen_addr.as_str()),
        config.spa_enabled.then_some(config.spa_bind.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        let address: SocketAddr =
            listener_policy::bind_address(raw, config.unsupported_allow_remote_listeners)?
                .parse()?;
        if address.port() != 0 && owned.is_some_and(|addresses| addresses.contains(&address)) {
            continue;
        }
        reservations.push(bind(address)?);
    }
    Ok(reservations)
}

pub(super) fn reserve_replacement(
    config: &ProxyConfig,
    status: Option<&RuntimeStatus>,
) -> Result<Reservations> {
    let owned = match status {
        Some(status) => {
            anyhow::ensure!(
                status.ready && !status.stopping,
                "current owner is initializing or draining; existing daemon was preserved"
            );
            Some(status.bound_listeners.as_deref().context(
                "current daemon lacks verified bound-listener facts; explicitly stop it and start it again to upgrade; automatic replacement preserved the daemon",
            )?)
        }
        None => None,
    };
    reserve(config, owned)
}

#[cfg(test)]
#[path = "listener_reservations_tests.rs"]
mod tests;
