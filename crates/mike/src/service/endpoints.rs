use super::spec::ServiceKind;
use crate::config::ProxyConfig;
use crate::doctor::connect_addr;

#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    pub(crate) proxy_addr: String,
    pub(crate) inspect_addr: String,
    pub(crate) spa_addr: Option<String>,
    pub(crate) sidecar_url: String,
}

impl Endpoints {
    pub(crate) fn from_config(config: &ProxyConfig) -> Self {
        Self {
            proxy_addr: connect_addr(&config.proxy_listen_addr),
            inspect_addr: connect_addr(&config.inspect_listen_addr),
            spa_addr: config.spa_enabled.then(|| connect_addr(&config.spa_bind)),
            sidecar_url: config.launch.proxy_forward_url.clone(),
        }
    }

    pub(crate) fn health_url(&self) -> String {
        format!("http://{}/api/health", self.inspect_addr)
    }

    pub(crate) fn addresses(&self, kind: ServiceKind) -> Vec<(&'static str, String)> {
        match kind {
            ServiceKind::Proxy => {
                let mut addresses = vec![
                    ("proxy", self.proxy_addr.clone()),
                    ("inspect", self.inspect_addr.clone()),
                ];
                if let Some(spa) = &self.spa_addr {
                    addresses.push(("web UI", spa.clone()));
                }
                addresses
            }
            ServiceKind::Sidecar => crate::forward::bind_of(&self.sidecar_url)
                .map(|bind| ("sidecar", connect_addr(&bind)))
                .into_iter()
                .collect(),
        }
    }
}
