pub(crate) mod channel_link_sweep;
pub(crate) mod compress;
pub(crate) mod db_cost;
pub(crate) mod db_pool;
pub(crate) mod launch;
pub(crate) mod lesson_relations;
pub(crate) mod load;
pub(crate) mod memory_cap;
pub(crate) mod memory_harvester;
pub mod recall_injector;
pub(crate) mod reminders;
pub(crate) mod retention_sweep;
pub(crate) mod session_judge;
pub(crate) mod steerer;
pub mod subcommands;

mod child;
mod credentials;
pub mod endpoint;
pub mod paths;
pub(crate) mod weasel_intercept;
pub mod write;

pub use child::configure_child;
pub(crate) use credentials::reload_openrouter_key;

pub(crate) fn env_text(name: &str) -> Option<String> {
    paths::live_install_guard::env(name).and_then(|value| value.into_string().ok())
}

pub(crate) fn env_text_from(
    environment: &dyn crate::environment::Environment,
    name: &str,
) -> Option<String> {
    paths::environment_value(&crate::environment::lookup(environment), name)
        .and_then(|value| value.into_string().ok())
}

mod bool_parse;
mod overrides;
mod overrides_analysis;
mod overrides_launch;
mod overrides_rerank;
pub(crate) mod persist_bootstrap;
mod proxy_config_defaults;
mod schema;
#[cfg(test)]
mod schema_tests;

pub use load::{CliOverrides, load, reload_selected};

pub(crate) use bool_parse::{deserialize_permissive_bool, parse_permissive_bool_str};
pub use launch::LaunchConfig;
pub(crate) use overrides::apply_settings_overrides;
pub(crate) use persist_bootstrap::{build_persist_handle, primary_tcp_unreachable};
pub use proxy_config_defaults::LessonWorkerConfig;
pub use schema::ProxyConfig;
pub use write_guards::{
    FeatureToggleWriteGuard, FilterWriteGuard, SettingsWriteGuard, SteeringWriteGuard,
};

#[cfg(unix)]
pub(crate) mod write_guards;
mod write_transaction;
