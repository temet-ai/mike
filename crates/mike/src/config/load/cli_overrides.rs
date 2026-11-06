use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Parser, Serialize, Deserialize)]
#[command(name = "mike", about = "Reverse proxy for Claude API traffic")]
#[serde(rename_all = "snake_case")]
pub struct CliOverrides {
    #[arg(long, global = true, value_name = "FILE")]
    #[serde(skip)]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, hide = true)]
    #[serde(skip)]
    pub boot_config_snapshot: Option<PathBuf>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_listen_addr: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspect_listen_addr: Option<String>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_allow_remote_listeners: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_url: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_requests: Option<usize>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otel_endpoint: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_format: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persist_enabled: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_database_url: Option<String>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_fallback_enabled: Option<bool>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_resync_enabled: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<PathBuf>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_pinned: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_ttl_secs: Option<u64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_ttl: Option<String>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sweep_at_startup: Option<bool>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smoke_run: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontend_port: Option<u16>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spa_enabled: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spa_bind: Option<String>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composition_enabled: Option<bool>,

    #[arg(long = "embedding", value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_enabled: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_request_body_bytes: Option<u64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_response_body_bytes: Option<u64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_in_flight: Option<u64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_store_bytes: Option<u64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_stale_minutes: Option<i64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_delivery_ttl_minutes: Option<i64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_max_deliveries_per_turn: Option<i64>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_reject_send_to_ended_session: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_backend: Option<String>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_backend_follow_caller: Option<bool>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_use_sidecar: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_proxy_forward_url: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_no_proxy: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_ca_cert: Option<String>,

    #[arg(long, value_delimiter = ' ', num_args = 1, allow_hyphen_values = true)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_claude_args: Option<Vec<String>>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_brief_inline_max: Option<usize>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_channel_check_enabled: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_channel_check_timeout_secs: Option<u64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_channel_check_interval_secs: Option<u64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_warn_surfaces: Option<u32>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_max_surfaces: Option<u32>,

    #[arg(long, value_parser = crate::config::parse_permissive_bool_str)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_auto_trust: Option<bool>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_kill_wait_secs: Option<u64>,

    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_worktree_config: Option<String>,
}

impl std::fmt::Debug for CliOverrides {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliOverrides")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
