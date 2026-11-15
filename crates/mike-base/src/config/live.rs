use std::sync::Arc;

use arc_swap::ArcSwap;

use super::ProxyConfig;
use super::overrides::apply_settings_overrides;

pub struct LiveConfig {
    base: ArcSwap<ProxyConfig>,
    current: ArcSwap<ProxyConfig>,
}

impl LiveConfig {
    pub fn new(base: ProxyConfig, folded: ProxyConfig) -> Self {
        Self {
            base: ArcSwap::from_pointee(base),
            current: ArcSwap::from_pointee(folded),
        }
    }

    pub fn unfolded(config: ProxyConfig) -> Self {
        Self::new(config.clone(), config)
    }

    pub fn load(&self) -> Arc<ProxyConfig> {
        self.current.load_full()
    }

    pub fn refold(&self, rows: &[(String, serde_json::Value)]) {
        let mut next = (*self.base.load_full()).clone();
        apply_settings_overrides(&mut next, rows);
        self.current.store(Arc::new(next));
    }

    pub fn edit_boot_values(&self, edit: impl Fn(&mut ProxyConfig)) {
        self.base.rcu(|base| {
            let mut next = (**base).clone();
            edit(&mut next);
            next
        });
        self.current.rcu(|current| {
            let mut next = (**current).clone();
            edit(&mut next);
            next
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refold_changed_row_updates_current_without_touching_base() {
        let base = ProxyConfig::default();
        let before = base.mailbox_stale_minutes;
        let live = LiveConfig::unfolded(base);
        live.refold(&[(
            "mailbox_stale_minutes".to_owned(),
            serde_json::Value::from(before + 7),
        )]);
        assert_eq!(live.load().mailbox_stale_minutes, before + 7);
        live.refold(&[]);
        assert_eq!(live.load().mailbox_stale_minutes, before);
    }

    #[test]
    fn edit_boot_values_bound_address_survives_refold() {
        let live = LiveConfig::unfolded(ProxyConfig::default());
        live.edit_boot_values(|c| c.proxy_listen_addr = "127.0.0.1:1".to_owned());
        live.refold(&[]);
        assert_eq!(live.load().proxy_listen_addr, "127.0.0.1:1");
    }
}
