use serde::{Deserialize, Serialize};

use crate::analysis::AnalysisConfig;
use crate::config::memory_cap::CapManagerConfig;

use super::load;
use super::schema::ProxyConfig;

pub(crate) fn default_max_request_body_bytes() -> u64 {
    16 * 1024 * 1024
}

pub(crate) fn default_max_response_body_bytes() -> u64 {
    32 * 1024 * 1024
}

pub(crate) fn default_max_in_flight() -> u64 {
    500
}

pub(crate) fn default_max_store_bytes() -> u64 {
    3 * 1024 * 1024 * 1024
}

pub(crate) fn default_mailbox_stale_minutes() -> i64 {
    10
}

pub(crate) fn default_mailbox_redelivery_max_attempts() -> i32 {
    5
}

pub(crate) fn default_mailbox_redelivery_backoff_base_secs() -> i64 {
    180
}

pub(crate) fn default_mailbox_redelivery_backoff_cap_secs() -> i64 {
    900
}

pub(crate) fn default_mailbox_delivery_ttl_minutes() -> i64 {
    60
}

pub(crate) fn default_mailbox_max_deliveries_per_turn() -> i64 {
    5
}

pub(crate) fn default_workspace_reap_interval_secs() -> u64 {
    120
}

pub(crate) fn default_db_connect_timeout_ms() -> u64 {
    10_000
}

pub(crate) fn default_spa_bind() -> String {
    "127.0.0.1:9990".to_string()
}

pub(crate) fn default_spa_origins() -> Vec<String> {
    vec![
        "http://127.0.0.1:9990".to_string(),
        "http://localhost:9990".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LessonWorkerConfig {
    pub enabled: bool,

    pub dedup_threshold: f32,

    pub max_lessons_per_project: i64,

    pub session_consolidation_idle_secs: u64,

    pub nightly_consolidation_hour: u8,

    pub gc_interval_secs: u64,
}

impl Default for LessonWorkerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dedup_threshold: 0.85,
            max_lessons_per_project: 100,
            session_consolidation_idle_secs: 300,
            nightly_consolidation_hour: 3,
            gc_interval_secs: 3600,
        }
    }
}

impl ProxyConfig {
    pub fn set_openrouter_base_url(&mut self, url: String) {
        self.steerer.openrouter_base_url = url;
    }

    pub fn set_openrouter_api_key(&mut self, key: String) {
        self.steerer.openrouter_api_key = Some(secrecy::SecretString::new(key.into()));
    }

    pub fn set_judge_enabled(&mut self, enabled: bool) {
        self.steerer.judge.enabled = enabled;
    }

    pub fn set_composition_enabled(&mut self, enabled: bool) {
        self.composition_enabled = enabled;
    }
}

impl ProxyConfig {
    pub fn try_parse_from<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let overrides = <load::CliOverrides as clap::Parser>::try_parse_from(args)?;
        load::load(overrides).map_err(|e| {
            clap::Error::raw(clap::error::ErrorKind::ValueValidation, format!("{e:#}\n"))
        })
    }

    #[doc(hidden)]
    pub fn for_tests_in_memory() -> Self {
        Self {
            persist_enabled: false,
            config_path: private_test_installation(),
            ..Self::default()
        }
    }
}

fn private_test_installation() -> std::path::PathBuf {
    const MAX_FOLDER_PATH_LEN: usize = 40;
    let mut builder = tempfile::Builder::new();
    builder.prefix("mike-test-");
    let preferred = builder.tempdir().unwrap_or_else(|error| {
        panic!("cannot create a private test installation folder: {error}")
    });
    let folder = if preferred.path().as_os_str().len() <= MAX_FOLDER_PATH_LEN {
        preferred
    } else {
        drop(preferred);
        builder.tempdir_in("/tmp").unwrap_or_else(|error| {
            panic!("cannot create a private test installation folder in /tmp: {error}")
        })
    };
    folder.keep().join("config.toml")
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            instance_id: String::new(),
            boot_generation: String::new(),
            config_path: std::path::PathBuf::new(),
            boot_overrides: super::CliOverrides::default(),
            proxy_listen_addr: "127.0.0.1:9988".to_string(),
            inspect_listen_addr: "127.0.0.1:9989".to_string(),
            unsupported_allow_remote_listeners: false,
            upstream_url: "https://api.anthropic.com".to_string(),
            max_requests: 1000,
            max_age: "24h".to_string(),
            otel_endpoint: None,
            log_format: "pretty".to_string(),
            log_level: "info".to_string(),
            persist_enabled: true,
            database_url: None,
            local_database_url: None,
            db_connect_timeout_ms: default_db_connect_timeout_ms(),
            db_pool: super::db_pool::DbPoolConfig::default(),
            db_fallback_enabled: true,
            db_resync_enabled: true,
            data_dir: None,
            project_pinned: None,
            default_ttl_secs: 3600,
            default_ttl: "365d".to_string(),
            sweep_at_startup: false,
            smoke_run: false,
            frontend_port: 9990,
            spa_enabled: true,
            spa_bind: default_spa_bind(),
            compress: crate::config::compress::CompressConfig::default(),
            reminders: crate::config::reminders::RemindersConfig::default(),
            weasel_intercept: crate::config::weasel_intercept::WeaselInterceptConfig::default(),
            composition_enabled: false,
            spa_origins: default_spa_origins(),
            inspect_allowed_hosts: Vec::new(),
            max_request_body_bytes: default_max_request_body_bytes(),
            max_response_body_bytes: default_max_response_body_bytes(),
            max_in_flight: default_max_in_flight(),
            max_store_bytes: default_max_store_bytes(),
            mailbox_stale_minutes: default_mailbox_stale_minutes(),
            mailbox_redelivery_max_attempts: default_mailbox_redelivery_max_attempts(),
            mailbox_redelivery_backoff_base_secs: default_mailbox_redelivery_backoff_base_secs(),
            mailbox_redelivery_backoff_cap_secs: default_mailbox_redelivery_backoff_cap_secs(),
            mailbox_delivery_ttl_minutes: default_mailbox_delivery_ttl_minutes(),
            mailbox_max_deliveries_per_turn: default_mailbox_max_deliveries_per_turn(),
            mailbox_reject_send_to_ended_session: true,
            workspace_reap_interval_secs: default_workspace_reap_interval_secs(),
            embedding: crate::analysis::EmbeddingConfig::default(),
            rerank: crate::analysis::RerankConfig::default(),
            analysis: AnalysisConfig::default(),
            cap_manager: CapManagerConfig::default(),
            log_dir: None,
            log_max_files: 7,
            lesson_worker: LessonWorkerConfig::default(),
            recall_injector: crate::config::recall_injector::RecallInjectorConfig::default(),
            harvester: crate::config::memory_harvester::HarvesterConfig::default(),
            steerer: crate::config::steerer::SteererConfig::default(),
            session_judge: crate::config::session_judge::SessionJudgeConfig::default(),
            retention_sweep: crate::config::retention_sweep::RetentionSweepConfig::default(),
            channel_link_sweep: crate::config::channel_link_sweep::ChannelLinkSweepConfig::default(
            ),
            db_cost: crate::config::db_cost::DbCostConfig::default(),
            lesson_relations: crate::config::lesson_relations::LessonRelationsConfig::default(),
            launch: super::launch::LaunchConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_config_has_composer_disabled() -> anyhow::Result<()> {
        let cfg = super::ProxyConfig::try_parse_from(["mike"])?;
        assert!(!cfg.composition_enabled);
        Ok(())
    }
}

#[cfg(test)]
#[path = "proxy_config_defaults_tests.rs"]
mod proxy_config_defaults_tests;
