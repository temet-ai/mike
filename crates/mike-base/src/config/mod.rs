pub mod channel_link_sweep;
pub mod compress;
pub mod db_cost;
pub mod db_fold;
pub mod db_pool;
pub mod launch;
pub mod lesson_relations;
pub mod load;
pub mod memory_cap;
pub mod memory_harvester;
pub mod recall_injector;
pub mod reminders;
pub mod retention_sweep;
pub mod session_judge;
pub mod steerer;

pub mod child;
pub mod credentials;
pub mod endpoint;
pub mod paths;
pub mod weasel_intercept;
pub mod write;

pub use child::configure_child;
pub use credentials::reload_openrouter_key;

pub fn env_text(name: &str) -> Option<String> {
    paths::live_install_guard::env(name).and_then(|value| value.into_string().ok())
}

pub fn env_text_from(
    environment: &dyn crate::environment::Environment,
    name: &str,
) -> Option<String> {
    paths::environment_value(&crate::environment::lookup(environment), name)
        .and_then(|value| value.into_string().ok())
}

pub mod bool_parse;
pub mod overrides;
pub mod overrides_analysis;
pub mod overrides_launch;
pub mod overrides_rerank;
pub mod persist_bootstrap;
pub mod proxy_config_defaults;
pub mod schema;
#[cfg(test)]
pub mod schema_tests;

pub use load::{CliOverrides, load, reload_selected};

pub use bool_parse::{deserialize_permissive_bool, parse_permissive_bool_str};
pub use launch::LaunchConfig;
pub use overrides::apply_settings_overrides;
pub use persist_bootstrap::{build_persist_handle, primary_tcp_unreachable};
pub use proxy_config_defaults::LessonWorkerConfig;
pub use schema::ProxyConfig;
pub use write_guards::{
    FeatureToggleWriteGuard, FilterWriteGuard, SettingsWriteGuard, SteeringWriteGuard,
};

#[cfg(unix)]
pub mod write_guards;
pub mod write_transaction;
