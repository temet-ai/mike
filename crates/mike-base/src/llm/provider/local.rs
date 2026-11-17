use super::SOURCE;
use crate::environment::{Environment, ProcessEnvironment};
use crate::settings::SettingsOverlay;

pub use crate::settings::local_llm::{DEFAULT_LOCAL_KEY_ENV, LOCAL_KEY_ENV_KEY, LOCAL_URL_KEY};
pub const LOCAL_BUSY: &str = "local server busy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEndpoint {
    pub url: String,
    pub api_key_env: String,
}

impl Default for LocalEndpoint {
    fn default() -> Self {
        Self {
            url: String::new(),
            api_key_env: DEFAULT_LOCAL_KEY_ENV.to_owned(),
        }
    }
}

impl LocalEndpoint {
    pub fn from_overlay(overlay: &SettingsOverlay) -> Self {
        Self {
            url: overlay.llm_local_url.clone(),
            api_key_env: overlay.llm_local_api_key_env.clone(),
        }
    }

    pub fn base_url(&self) -> Option<&str> {
        let url = self.url.trim().trim_end_matches('/');
        (!url.is_empty()).then_some(url)
    }

    pub fn slots_url(&self) -> Option<String> {
        let base = self.base_url()?;
        let root = base.strip_suffix("/v1").unwrap_or(base);
        Some(format!("{root}/slots"))
    }

    pub fn api_key(&self) -> String {
        self.api_key_from(&ProcessEnvironment)
    }

    pub fn api_key_from(&self, environment: &dyn Environment) -> String {
        let name = self.api_key_env.trim();
        if name.is_empty() {
            return String::new();
        }
        environment.var(name).unwrap_or_default()
    }
}

pub fn local_endpoint() -> LocalEndpoint {
    match SOURCE.load_full() {
        Some(overlay) => LocalEndpoint::from_overlay(&overlay.load()),
        None => LocalEndpoint::default(),
    }
}

#[cfg(test)]
pub fn install_endpoint_for_tests(url: &str, api_key_env: &str) {
    let overlay = SettingsOverlay {
        llm_local_url: url.to_owned(),
        llm_local_api_key_env: api_key_env.to_owned(),
        ..SettingsOverlay::default()
    };
    super::install(std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
        overlay,
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::MapEnvironment;

    fn endpoint(url: &str) -> LocalEndpoint {
        LocalEndpoint {
            url: url.to_owned(),
            api_key_env: "LOCAL_KEY".to_owned(),
        }
    }

    #[test]
    fn api_key_from_reads_the_named_variable_and_defaults_to_empty() {
        let environment = MapEnvironment::default().with("LOCAL_KEY", "sk-local");
        assert_eq!(endpoint("http://h").api_key_from(&environment), "sk-local");
        assert_eq!(
            endpoint("http://h").api_key_from(&MapEnvironment::default()),
            ""
        );
    }

    #[test]
    fn base_url_empty_means_not_configured_and_trailing_slash_is_trimmed() {
        assert_eq!(endpoint("  ").base_url(), None);
        assert_eq!(
            endpoint("http://h:8080/v1/").base_url(),
            Some("http://h:8080/v1")
        );
    }

    #[test]
    fn slots_url_drops_the_v1_suffix() {
        assert_eq!(
            endpoint("http://h:8080/v1").slots_url().as_deref(),
            Some("http://h:8080/slots")
        );
        assert_eq!(
            endpoint("http://h:8080").slots_url().as_deref(),
            Some("http://h:8080/slots")
        );
        assert_eq!(endpoint("").slots_url(), None);
    }
}
