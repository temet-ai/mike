use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyEnv {
    pub https_proxy: String,
    pub no_proxy: String,
    pub ca_cert: PathBuf,
}

impl ProxyEnv {
    pub fn variables(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("HTTPS_PROXY".to_string(), self.https_proxy.clone()),
            ("NO_PROXY".to_string(), self.no_proxy.clone()),
            (
                "NODE_EXTRA_CA_CERTS".to_string(),
                self.ca_cert.to_string_lossy().into_owned(),
            ),
        ])
    }
}

pub fn surface_env(proxy: Option<&ProxyEnv>, proxy_env_enabled: bool) -> BTreeMap<String, String> {
    match proxy {
        Some(p) if proxy_env_enabled => p.variables(),
        _ => BTreeMap::new(),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn proxy() -> ProxyEnv {
        ProxyEnv {
            https_proxy: "http://127.0.0.1:9987".into(),
            no_proxy: "localhost,127.0.0.1".into(),
            ca_cert: "/repo/.deploy/proxy-forward-ca.pem".into(),
        }
    }

    #[test]
    fn variables_carry_the_three_contract_names() {
        let vars = proxy().variables();
        assert_eq!(vars["HTTPS_PROXY"], "http://127.0.0.1:9987");
        assert_eq!(vars["NO_PROXY"], "localhost,127.0.0.1");
        assert_eq!(
            vars["NODE_EXTRA_CA_CERTS"],
            "/repo/.deploy/proxy-forward-ca.pem"
        );
        assert_eq!(vars.len(), 3);
    }

    #[test]
    fn surface_env_no_proxy_env_flag_yields_empty() {
        assert!(surface_env(Some(&proxy()), false).is_empty());
        assert!(surface_env(None, true).is_empty());
        assert_eq!(surface_env(Some(&proxy()), true).len(), 3);
    }
}
