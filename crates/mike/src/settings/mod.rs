pub mod service;

pub(crate) mod live_bundle_follow;

mod llm_runtime;
mod meta;
mod meta_assistants;
mod meta_cross_session;
pub(crate) mod meta_dossier;
pub(crate) mod meta_eval;
pub(crate) mod meta_eval_alarms;
mod meta_global;
mod meta_improvement;
mod meta_launch;
mod meta_llm_budget;
mod meta_llm_provider;
mod meta_operations;
mod meta_pipeline;
mod meta_rerank;
mod meta_rerank_nightly;
#[cfg(test)]
mod meta_rerank_tests;
pub(crate) mod meta_scheduled_jobs;
mod meta_search_ranking;
#[cfg(test)]
mod meta_search_ranking_tests;
pub(crate) mod meta_semantic;
mod meta_session_end;
mod meta_session_scoped;
mod meta_shell_output;
pub(crate) mod meta_turn_guard;
pub(crate) mod meta_turn_judge;
pub(crate) mod meta_work_registry;
mod meta_workers;
mod overlay_apply;
mod overlay_apply_provider;
mod overlay_apply_session_end;
mod overlay_apply_turn_judge;
pub(crate) mod running_services;
#[cfg(test)]
mod running_services_tests;
#[cfg(test)]
mod settings_tests;
mod validate;

pub(crate) use llm_runtime::{
    SharedSettingsOverlay, resolve_judge_worker_llm_settings, resolve_session_judge_llm_settings,
    resolve_shared_llm_settings, resolve_steerer_llm_settings, session_judge_live_source,
    steerer_live_source,
};
pub use meta::SETTINGS_META;
pub(crate) use overlay_apply::apply_db_rows_to_overlay;
pub(crate) use validate::validate_setting_value;

use serde::{Deserialize, Serialize};

use crate::weasel_intercept::config::DEFAULT_CORRECTION_PROMPT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Bool,
    Int,
    Float,
    String,
    Text,
    StringList,
    Enum,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingScope {
    Session,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingApplies {
    NewSessions,
    OnRestart,
    NextCycle,
    NextMessage,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingMeta {
    pub key: &'static str,
    pub group: &'static str,
    pub kind: SettingKind,
    pub scope: SettingScope,
    pub applies: SettingApplies,
    pub default: serde_json::Value,
    pub label: &'static str,
    pub description: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_i64: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_i64: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_f64: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_f64: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_len: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_variants: Option<&'static [&'static str]>,
}

pub(crate) const DEFAULT_PROTECTED_PATHS: &[&str] = &[
    "**/CLAUDE.md",
    "**/BRIEF*.md",
    "**/.claude/**",
    "**/docs/plans/**",
    "**/artifacts/threads/**",
    "**/*.rules.md",
];

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsOverlay {
    pub reminders_inject_lessons: bool,

    pub reminders_max_lessons_to_inject: usize,

    pub weasel_intercept_forbidden_words: Vec<String>,

    pub weasel_intercept_correction_prompt: String,

    pub recall_injector_max_lessons_per_topic: usize,

    pub file_read_compression_min_bytes: usize,

    pub file_read_compression_head_lines: usize,

    pub file_read_compression_tail_lines: usize,

    pub file_read_compression_enabled_producers: Vec<String>,

    pub file_read_compression_protected_paths: Vec<String>,

    pub file_read_compression_grace_turns: usize,

    pub file_read_compression_mode: String,

    pub file_read_compression_forward_share: usize,

    pub shell_output_compression_min_bytes: usize,

    pub shell_output_compression_head_lines: usize,

    pub shell_output_compression_tail_lines: usize,

    pub shell_output_compression_signal_context_lines: usize,

    pub shell_output_compression_signal_line_cap: usize,

    pub shell_output_compression_grace_turns: usize,

    pub shell_output_compression_mode: String,

    pub shell_output_compression_forward_share: usize,

    pub compression_dedup_enabled: bool,

    pub context_focus_question_mode: String,

    pub judge_worker_model: Option<String>,

    pub judge_worker_system_prompt: Option<String>,

    pub judge_worker_batch_size: Option<i64>,

    pub judge_worker_confidence_threshold: Option<f32>,

    pub session_judge_model: Option<String>,

    pub session_judge_system_prompt: Option<String>,

    pub session_judge_batch_size: Option<i64>,

    pub session_judge_min_session_age_secs: Option<u64>,

    pub session_judge_max_turns_in_prompt: Option<usize>,

    pub session_judge_max_chars_per_turn: i64,

    pub session_judge_max_attempts_per_day: Option<i64>,

    pub health_llm_failures_degrade_status: bool,

    pub llm_reasoning_effort: String,

    pub llm_max_output_tokens: i64,

    pub llm_rate_limit_per_minute: i64,

    pub llm_ceiling_max_calls: i64,

    pub llm_ceiling_window_secs: i64,

    pub llm_daily_spend_ceiling_usd: f64,

    pub llm_spend_alert_fraction: f64,

    pub llm_record_usage: bool,

    pub llm_budget_steerer_share_pct: i64,

    pub llm_budget_session_judge_share_pct: i64,

    pub llm_budget_judge_worker_share_pct: i64,

    pub llm_budget_lesson_relations_share_pct: i64,
    pub llm_budget_lesson_proposer_share_pct: i64,

    pub llm_budget_eval_share_pct: i64,

    pub llm_budget_other_share_pct: i64,

    pub llm_provider: std::sync::Arc<crate::llm::provider::ProviderPolicy>,

    pub steerer_model: Option<String>,

    pub steerer_reasoning_effort: String,

    pub steerer_max_output_tokens: i64,

    pub steerer_retain_pass_verdicts: bool,

    pub steerer_session_note_limit: i64,

    pub steerer_session_note_window_minutes: i64,

    pub steerer_wrong_note_mute_hours: i64,

    pub steerer_verifier_input_budget: String,

    pub steerer_intake_criteria_input_budget: String,

    pub session_judge_reasoning_effort: String,

    pub session_judge_max_output_tokens: i64,

    pub judge_worker_reasoning_effort: String,

    pub judge_worker_max_output_tokens: i64,

    pub turn_judge_live_bundle_id: i64,

    pub session_judge_live_bundle_id: i64,

    pub session_judge_v3_lookback_hours: i64,

    pub session_judge_rejudge_daily_call_cap: i64,

    pub session_end_shadow_bundle_id: i64,

    pub session_end_shadow_sample_rate: f64,

    pub session_end_shadow_batch_size: i64,

    pub session_end_shadow_lookback_hours: i64,

    pub session_end_shadow_interval_secs: i64,

    pub turn_judge_immediate: bool,

    pub turn_judge_immediate_delay_ms: i64,

    pub turn_judge_live_window_days: i64,

    pub turn_judge_rejudge_daily_call_cap: i64,

    pub bundles: std::sync::Arc<crate::eval::bundles::BundleIds>,
}

impl Default for SettingsOverlay {
    fn default() -> Self {
        Self {
            reminders_inject_lessons: false,
            reminders_max_lessons_to_inject: 5,
            weasel_intercept_forbidden_words: Vec::new(),
            weasel_intercept_correction_prompt: DEFAULT_CORRECTION_PROMPT.to_owned(),
            recall_injector_max_lessons_per_topic: 3,
            file_read_compression_min_bytes: 1500,
            file_read_compression_head_lines: 40,
            file_read_compression_tail_lines: 20,
            file_read_compression_enabled_producers: vec!["Read".to_owned()],
            file_read_compression_protected_paths: DEFAULT_PROTECTED_PATHS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            file_read_compression_grace_turns: 3,
            file_read_compression_mode: "shadow".to_owned(),
            file_read_compression_forward_share: 0,
            shell_output_compression_min_bytes:
                crate::compress::shell_output::DEFAULT_MIN_RESULT_BYTES,
            shell_output_compression_head_lines: crate::compress::shell_output::DEFAULT_HEAD_LINES,
            shell_output_compression_tail_lines: crate::compress::shell_output::DEFAULT_TAIL_LINES,
            shell_output_compression_signal_context_lines:
                crate::compress::shell_output::DEFAULT_SIGNAL_CONTEXT_LINES,
            shell_output_compression_signal_line_cap:
                crate::compress::shell_output::DEFAULT_SIGNAL_LINE_CAP,
            shell_output_compression_grace_turns:
                crate::compress::shell_output::DEFAULT_GRACE_TURNS,
            shell_output_compression_mode: "shadow".to_owned(),
            shell_output_compression_forward_share: 0,
            compression_dedup_enabled: false,
            context_focus_question_mode: "plain".to_owned(),
            judge_worker_model: None,
            judge_worker_system_prompt: None,
            judge_worker_batch_size: None,
            judge_worker_confidence_threshold: None,
            session_judge_model: None,
            session_judge_system_prompt: None,
            session_judge_batch_size: None,
            session_judge_min_session_age_secs: None,
            session_judge_max_turns_in_prompt: None,
            session_judge_max_chars_per_turn: 3_000,
            session_judge_max_attempts_per_day: None,
            health_llm_failures_degrade_status: false,
            llm_reasoning_effort: "none".to_owned(),
            llm_max_output_tokens: 800,
            llm_rate_limit_per_minute: 12,
            llm_ceiling_max_calls: 100,
            llm_ceiling_window_secs: 3600,
            llm_daily_spend_ceiling_usd: 0.0,
            llm_spend_alert_fraction: 0.75,
            llm_record_usage: true,
            llm_budget_steerer_share_pct: 25,
            llm_budget_session_judge_share_pct: 20,
            llm_budget_judge_worker_share_pct: 25,
            llm_budget_lesson_relations_share_pct: 5,
            llm_budget_lesson_proposer_share_pct: 10,
            llm_budget_eval_share_pct: 0,
            llm_budget_other_share_pct: 10,
            llm_provider: std::sync::Arc::new(crate::llm::provider::ProviderPolicy::default()),
            steerer_model: None,
            steerer_reasoning_effort: "inherit".to_owned(),
            steerer_max_output_tokens: 0,
            steerer_retain_pass_verdicts: true,
            steerer_session_note_limit: 3,
            steerer_session_note_window_minutes: 60,
            steerer_wrong_note_mute_hours: 24,
            steerer_verifier_input_budget: "verifier-input-v1".to_owned(),
            steerer_intake_criteria_input_budget: "intake-criteria-input-v1".to_owned(),
            session_judge_reasoning_effort: "inherit".to_owned(),
            session_judge_max_output_tokens: 0,
            judge_worker_reasoning_effort: "low".to_owned(),
            judge_worker_max_output_tokens: 0,
            turn_judge_live_bundle_id: 0,
            session_judge_live_bundle_id: 0,
            session_judge_v3_lookback_hours: 72,
            session_judge_rejudge_daily_call_cap: 200,
            session_end_shadow_bundle_id: 0,
            session_end_shadow_sample_rate: 0.2,
            session_end_shadow_batch_size: 10,
            session_end_shadow_lookback_hours: 72,
            session_end_shadow_interval_secs: 300,
            turn_judge_immediate: false,
            turn_judge_immediate_delay_ms: meta_turn_judge::DEFAULT_IMMEDIATE_DELAY_MS,
            turn_judge_live_window_days: meta_turn_judge::DEFAULT_LIVE_WINDOW_DAYS,
            turn_judge_rejudge_daily_call_cap: meta_turn_judge::DEFAULT_REJUDGE_DAILY_CALL_CAP,
            bundles: std::sync::Arc::new(crate::eval::bundles::BundleIds::default()),
        }
    }
}

impl SettingsOverlay {
    pub(crate) fn live_steerer_model(&self, boot_model: &str) -> String {
        self.steerer_model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .unwrap_or(boot_model)
            .to_owned()
    }
}
