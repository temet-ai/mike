use anyhow::Context;
use axum::Router;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::app::{inspect_router_on, proxy_router, spa_router_on};
use crate::state::AppState;
use crate::store::persistence::pool_classes::PoolClass;

pub(super) struct ServeTargets {
    pub(super) proxy_listener: TcpListener,
    pub(super) inspect_listener: TcpListener,
    pub(super) spa_listener: Option<TcpListener>,
    pub(super) proxy_app: Router,
    pub(super) inspect_app: Router,
    pub(super) spa_app: Router,
    pub(super) shutdown_token: CancellationToken,
}

impl ServeTargets {
    pub(super) fn bound_addresses(&self) -> anyhow::Result<Vec<std::net::SocketAddr>> {
        let mut addresses = vec![
            self.proxy_listener.local_addr()?,
            self.inspect_listener.local_addr()?,
        ];
        if let Some(listener) = &self.spa_listener {
            addresses.push(listener.local_addr()?);
        }
        Ok(addresses)
    }
}

pub(super) async fn bind_and_log(mut state: AppState) -> anyhow::Result<ServeTargets> {
    super::listener_policy::validate(state.config())?;
    let allow_remote = state.config().unsupported_allow_remote_listeners;
    let proxy_addr =
        super::listener_policy::bind_address(&state.config().proxy_listen_addr, allow_remote)?;
    let inspect_addr =
        super::listener_policy::bind_address(&state.config().inspect_listen_addr, allow_remote)?;
    let spa_enabled = state.config().spa_enabled;
    let spa_addr = if spa_enabled {
        super::listener_policy::bind_address(&state.config().spa_bind, allow_remote)?
    } else {
        state.config().spa_bind.clone()
    };
    let shutdown_token = state.shutdown();

    let proxy_listener = TcpListener::bind(&proxy_addr)
        .await
        .with_context(|| format!("failed to bind proxy to {proxy_addr}"))?;
    let inspect_listener = TcpListener::bind(&inspect_addr)
        .await
        .with_context(|| format!("failed to bind inspect API to {inspect_addr}"))?;
    let spa_listener = bind_spa_listener(spa_enabled, &spa_addr).await?;

    publish_bound_addresses(
        &mut state,
        &proxy_listener,
        &inspect_listener,
        spa_listener.as_ref(),
    )?;
    let proxy_addr = state.config().proxy_listen_addr.clone();
    let inspect_addr = state.config().inspect_listen_addr.clone();
    let spa_addr = state.config().spa_bind.clone();

    let proxy_app = proxy_router(state.on_pool_class(PoolClass::Capture));
    let interactive = state.on_pool_class(PoolClass::Interactive);
    let inspect_app = inspect_router_on(interactive.clone(), inspect_listener.local_addr()?);
    let spa_app = match &spa_listener {
        Some(listener) => spa_router_on(interactive, listener.local_addr()?),
        None => Router::new(),
    };

    log_bind_addresses(&proxy_addr, &inspect_addr, spa_enabled, &spa_addr);

    Ok(ServeTargets {
        proxy_listener,
        inspect_listener,
        spa_listener,
        proxy_app,
        inspect_app,
        spa_app,
        shutdown_token,
    })
}

fn publish_bound_addresses(
    state: &mut AppState,
    proxy: &TcpListener,
    inspect: &TcpListener,
    spa: Option<&TcpListener>,
) -> anyhow::Result<()> {
    let mut config = state.config().clone();
    config.proxy_listen_addr = proxy.local_addr()?.to_string();
    config.inspect_listen_addr = inspect.local_addr()?.to_string();
    if let Some(spa) = spa {
        config.spa_bind = spa.local_addr()?.to_string();
    }
    state.config = std::sync::Arc::new(config);
    Ok(())
}

fn log_bind_addresses(proxy_addr: &str, inspect_addr: &str, spa_enabled: bool, spa_addr: &str) {
    log_proxy_bind_listening(proxy_addr);
    log_inspect_bind_listening(inspect_addr);
    if spa_enabled {
        log_spa_bind_listening(spa_addr);
    } else {
        log_spa_bind_disabled();
    }
}

fn log_proxy_bind_listening(proxy_addr: &str) {
    info!(%proxy_addr, "mike proxy bind listening");
}

fn log_inspect_bind_listening(inspect_addr: &str) {
    info!(%inspect_addr, "mike inspect bind listening");
}

fn log_spa_bind_listening(spa_addr: &str) {
    info!(%spa_addr, "mike SPA bind listening");
}

fn log_spa_bind_disabled() {
    info!("mike SPA bind disabled (MIKE_SPA_ENABLED=0)");
}

async fn bind_spa_listener(enabled: bool, addr: &str) -> anyhow::Result<Option<TcpListener>> {
    if !enabled {
        return Ok(None);
    }
    Ok(Some(TcpListener::bind(addr).await.with_context(|| {
        format!("failed to bind SPA host to {addr}")
    })?))
}
