use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::analysis::AnalysisConfig;
use crate::config::memory_cap::CapManagerConfig;

use super::bool_parse::{deserialize_permissive_bool, deserialize_permissive_optional_bool};
use super::proxy_config_defaults::{
    default_db_connect_timeout_ms, default_mailbox_delivery_ttl_minutes,
    default_mailbox_max_deliveries_per_turn, default_mailbox_redelivery_backoff_base_secs,
    default_mailbox_redelivery_backoff_cap_secs, default_mailbox_redelivery_max_attempts,
    default_mailbox_stale_minutes, default_max_in_flight, default_max_request_body_bytes,
    default_max_response_body_bytes, default_max_store_bytes, default_spa_bind,
    default_spa_origins, default_workspace_reap_interval_secs,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyConfig {
    #[serde(skip)]
    pub instance_id: String,

    #[serde(skip)]
    pub boot_generation: String,

    #[serde(skip)]
    pub config_path: PathBuf,

    #[serde(skip)]
    pub boot_overrides: super::CliOverrides,

    pub proxy_listen_addr: String,

    pub inspect_listen_addr: String,

    #[serde(default, deserialize_with = "deserialize_permissive_bool")]
    pub unsupported_allow_remote_listeners: bool,

    pub upstream_url: String,

    pub max_requests: usize,

    pub max_age: String,

    pub otel_endpoint: Option<String>,

    pub log_format: String,

    pub log_level: String,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub persist_enabled: bool,

    pub database_url: Option<String>,

    pub local_database_url: Option<String>,

    #[serde(default = "default_db_connect_timeout_ms")]
    pub db_connect_timeout_ms: u64,

    #[serde(flatten)]
    pub db_pool: super::db_pool::DbPoolConfig,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub db_fallback_enabled: bool,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub db_resync_enabled: bool,

    pub data_dir: Option<PathBuf>,

    #[serde(default, deserialize_with = "deserialize_permissive_optional_bool")]
    pub project_pinned: Option<bool>,

    pub default_ttl_secs: u64,

    pub default_ttl: String,

    #[serde(default)]
    pub sweep_at_startup: bool,

    #[serde(default, deserialize_with = "deserialize_permissive_bool")]
    pub smoke_run: bool,

    pub frontend_port: u16,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub spa_enabled: bool,

    #[serde(default = "default_spa_bind")]
    pub spa_bind: String,

    #[serde(default)]
    pub compress: crate::config::compress::CompressConfig,

    #[serde(default)]
    pub reminders: crate::config::reminders::RemindersConfig,

    #[serde(default)]
    pub weasel_intercept: crate::config::weasel_intercept::WeaselInterceptConfig,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub composition_enabled: bool,

    #[serde(default = "default_spa_origins")]
    pub spa_origins: Vec<String>,

    #[serde(default)]
    pub inspect_allowed_hosts: Vec<String>,

    #[serde(default = "default_max_request_body_bytes")]
    pub max_request_body_bytes: u64,

    #[serde(default = "default_max_response_body_bytes")]
    pub max_response_body_bytes: u64,

    #[serde(default = "default_max_in_flight")]
    pub max_in_flight: u64,

    #[serde(default = "default_max_store_bytes")]
    pub max_store_bytes: u64,

    #[serde(default = "default_mailbox_stale_minutes")]
    pub mailbox_stale_minutes: i64,

    #[serde(default = "default_mailbox_redelivery_max_attempts")]
    pub mailbox_redelivery_max_attempts: i32,

    #[serde(default = "default_mailbox_redelivery_backoff_base_secs")]
    pub mailbox_redelivery_backoff_base_secs: i64,

    #[serde(default = "default_mailbox_redelivery_backoff_cap_secs")]
    pub mailbox_redelivery_backoff_cap_secs: i64,

    #[serde(default = "default_mailbox_delivery_ttl_minutes")]
    pub mailbox_delivery_ttl_minutes: i64,

    #[serde(default = "default_mailbox_max_deliveries_per_turn")]
    pub mailbox_max_deliveries_per_turn: i64,

    #[serde(deserialize_with = "deserialize_permissive_bool")]
    pub mailbox_reject_send_to_ended_session: bool,

    #[serde(default = "default_workspace_reap_interval_secs")]
    pub workspace_reap_interval_secs: u64,

    #[serde(default)]
    pub embedding: crate::analysis::EmbeddingConfig,

    #[serde(default)]
    pub rerank: crate::analysis::RerankConfig,

    #[serde(default)]
    pub analysis: AnalysisConfig,

    #[serde(default)]
    pub cap_manager: CapManagerConfig,

    pub log_dir: Option<PathBuf>,

    pub log_max_files: usize,

    #[serde(default)]
    pub steerer: crate::config::steerer::SteererConfig,

    #[serde(default)]
    pub session_judge: crate::config::session_judge::SessionJudgeConfig,

    #[serde(default)]
    pub retention_sweep: crate::config::retention_sweep::RetentionSweepConfig,

    #[serde(default)]
    pub db_cost: crate::config::db_cost::DbCostConfig,

    #[serde(flatten)]
    pub launch: super::launch::LaunchConfig,

    #[serde(flatten)]
    pub sections: super::sections::ConfigSections,
}

impl std::fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyConfig")
            .field("config_path", &self.config_path)
            .finish_non_exhaustive()
    }
}

impl ProxyConfig {
    pub fn max_requests(&self) -> usize {
        self.max_requests
    }

    pub fn max_age(&self) -> &str {
        &self.max_age
    }
}
