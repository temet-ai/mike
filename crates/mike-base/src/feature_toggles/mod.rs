pub mod meta;
#[cfg(test)]
pub mod native_peer_mail_tests;
pub mod service;

pub use meta::{TOGGLE_META, ToggleMeta};
#[cfg(test)]
mod tests;
#[cfg(test)]
pub mod tests_per_toggle;

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::settings::SettingsOverlay;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureToggles {
    pub reminders: bool,

    pub recall_injector: bool,

    pub weasel_intercept: bool,

    pub reminder_on_token: bool,

    pub mailbox_redelivery: bool,

    pub steerer: bool,

    pub lesson_worker: bool,

    pub harvester: bool,

    pub db_resync: bool,

    pub judge_worker: bool,

    pub session_judge: bool,

    pub agent_lessons: bool,

    pub file_read_compression: bool,

    pub shell_output_compression: bool,

    pub retention_sweep: bool,

    #[serde(default)]
    pub codex_diagnostic_capture: bool,

    pub channel_link_sweep: bool,

    pub proxy_tools: bool,

    pub workspace_capture_registration: bool,

    pub self_identity_reminder: bool,

    pub workspace_briefing: bool,

    pub repeated_notices: bool,

    /// Deliver a spawned session's long brief as mail from mike (pushed or
    /// channel-delivered once, else injected until acknowledged) instead of a
    /// file. Read by `mike spawn open` and `spawn_session` at each launch.
    /// Default off: it adds to the spawned session's traffic.
    pub launch_brief: bool,

    pub native_peer_mail: bool,

    pub mod_lesson_delivery: bool,

    pub fleet_auto_start: bool,

    pub prompt_assistant: bool,

    pub dossier_builder: bool,

    pub semantic_context: bool,

    pub turn_guard: bool,

    pub directive_proposer: bool,

    pub session_end_shadow: bool,

    pub judge_lessons: bool,

    pub change_record: bool,

    pub change_record_tail: bool,

    #[serde(default = "on")]
    pub code_scan: bool,

    #[serde(default)]
    pub code_findings: bool,

    #[serde(default = "on")]
    pub code_findings_push: bool,

    #[serde(default)]
    pub context_focus_question: bool,

    #[serde(default)]
    pub memory_keeper: bool,

    #[serde(default)]
    pub seat_memory_index: bool,

    #[serde(default)]
    pub wright: bool,

    #[serde(default = "on")]
    pub client_serves_self_identity: bool,
    #[serde(default)]
    pub client_serves_workspace_briefing: bool,
    #[serde(default)]
    pub client_serves_change_record_tail: bool,
    #[serde(default)]
    pub client_serves_reminders: bool,
    #[serde(default)]
    pub client_serves_recall: bool,
    #[serde(default)]
    pub client_serves_agent_lessons: bool,
    #[serde(default)]
    pub client_serves_prompt_shaping: bool,
    #[serde(default)]
    pub client_serves_proxy_tools: bool,
    #[serde(default)]
    pub client_serves_weasel_guard: bool,
    #[serde(default)]
    pub client_serves_mail: bool,
    #[serde(default)]
    pub client_serves_git_read_hint: bool,
    #[serde(default)]
    pub client_serves_memory_keeper: bool,
}

fn on() -> bool {
    true
}

impl Default for FeatureToggles {
    fn default() -> Self {
        Self {
            reminders: false,
            recall_injector: false,
            weasel_intercept: false,
            reminder_on_token: false,
            mailbox_redelivery: false,
            steerer: false,
            lesson_worker: true,
            harvester: true,
            db_resync: true,
            judge_worker: false,
            session_judge: false,
            agent_lessons: false,
            file_read_compression: false,
            shell_output_compression: false,
            retention_sweep: true,
            codex_diagnostic_capture: false,
            channel_link_sweep: true,
            proxy_tools: false,
            workspace_capture_registration: true,
            self_identity_reminder: false,
            workspace_briefing: false,
            repeated_notices: false,
            launch_brief: true,
            native_peer_mail: false,
            mod_lesson_delivery: false,
            fleet_auto_start: true,
            prompt_assistant: true,
            dossier_builder: false,
            semantic_context: true,
            turn_guard: false,
            directive_proposer: false,
            session_end_shadow: false,
            judge_lessons: false,
            change_record: false,
            change_record_tail: false,
            code_scan: true,
            code_findings: false,
            code_findings_push: true,
            context_focus_question: false,
            memory_keeper: false,
            seat_memory_index: false,
            wright: false,
            client_serves_self_identity: true,
            client_serves_workspace_briefing: false,
            client_serves_change_record_tail: false,
            client_serves_reminders: false,
            client_serves_recall: false,
            client_serves_agent_lessons: false,
            client_serves_prompt_shaping: false,
            client_serves_proxy_tools: false,
            client_serves_weasel_guard: false,
            client_serves_mail: false,
            client_serves_git_read_hint: false,
            client_serves_memory_keeper: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedToggles {
    pub reminders: bool,
    pub recall_injector: bool,
    pub weasel_intercept: bool,
    pub agent_lessons: bool,
    pub file_read_compression: bool,
    pub shell_output_compression: bool,
    pub proxy_tools: bool,
    pub context_focus_question: bool,
    pub workspace_capture_registration: bool,
    pub self_identity_reminder: bool,
    pub workspace_briefing: bool,
    pub repeated_notices: bool,
    pub features: crate::settings::FeatureOverlays,
}

impl ResolvedToggles {
    pub fn from_overlay(overlay: &FeatureToggles, settings: &SettingsOverlay) -> Self {
        Self {
            reminders: overlay.reminders,
            recall_injector: overlay.recall_injector,
            weasel_intercept: overlay.weasel_intercept,
            agent_lessons: overlay.agent_lessons,
            file_read_compression: overlay.file_read_compression,
            shell_output_compression: overlay.shell_output_compression,
            proxy_tools: overlay.proxy_tools,
            context_focus_question: overlay.context_focus_question,
            workspace_capture_registration: overlay.workspace_capture_registration,
            self_identity_reminder: overlay.self_identity_reminder,
            workspace_briefing: overlay.workspace_briefing,
            repeated_notices: overlay.repeated_notices,
            features: settings.features.clone(),
        }
    }
}

pub type SessionToggleCache = Arc<DashMap<String, Arc<ResolvedToggles>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToggleScope {
    Session,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToggleApplies {
    NewSessions,
    OnRestart,
    Immediate,
    LiveDisable,
}

pub fn get_toggle_enabled(toggles: &FeatureToggles, key: &str) -> Option<bool> {
    match key {
        "agent_lessons" => Some(toggles.agent_lessons),
        "change_record" => Some(toggles.change_record),
        "change_record_tail" => Some(toggles.change_record_tail),
        "code_findings" => Some(toggles.code_findings),
        "code_findings_push" => Some(toggles.code_findings_push),
        "code_scan" => Some(toggles.code_scan),
        "channel_link_sweep" => Some(toggles.channel_link_sweep),
        "context_focus_question" => Some(toggles.context_focus_question),
        "db_resync" => Some(toggles.db_resync),
        "dossier_builder" => Some(toggles.dossier_builder),
        "file_read_compression" => Some(toggles.file_read_compression),
        "shell_output_compression" => Some(toggles.shell_output_compression),
        "fleet_auto_start" => Some(toggles.fleet_auto_start),
        "harvester" => Some(toggles.harvester),
        "judge_lessons" => Some(toggles.judge_lessons),
        "judge_worker" => Some(toggles.judge_worker),
        "launch_brief" => Some(toggles.launch_brief),
        "lesson_worker" => Some(toggles.lesson_worker),
        "mailbox_redelivery" => Some(toggles.mailbox_redelivery),
        "memory_keeper" => Some(toggles.memory_keeper),
        "seat_memory_index" => Some(toggles.seat_memory_index),
        "wright" => Some(toggles.wright),
        "mod_lesson_delivery" => Some(toggles.mod_lesson_delivery),
        "native_peer_mail" => Some(toggles.native_peer_mail),
        "prompt_assistant" => Some(toggles.prompt_assistant),
        "proxy_tools" => Some(toggles.proxy_tools),
        "recall_injector" => Some(toggles.recall_injector),
        "reminder_on_token" => Some(toggles.reminder_on_token),
        "reminders" => Some(toggles.reminders),
        "repeated_notices" => Some(toggles.repeated_notices),
        "retention_sweep" => Some(toggles.retention_sweep),
        "codex_diagnostic_capture" => Some(toggles.codex_diagnostic_capture),
        "semantic_context" => Some(toggles.semantic_context),
        "turn_guard" => Some(toggles.turn_guard),
        "directive_proposer" => Some(toggles.directive_proposer),
        "self_identity_reminder" => Some(toggles.self_identity_reminder),
        "session_end_shadow" => Some(toggles.session_end_shadow),
        "session_judge" => Some(toggles.session_judge),
        "steerer" => Some(toggles.steerer),
        "weasel_intercept" => Some(toggles.weasel_intercept),
        "workspace_briefing" => Some(toggles.workspace_briefing),
        "workspace_capture_registration" => Some(toggles.workspace_capture_registration),
        _ => client_serves_mut(&mut toggles.clone(), key).map(|flag| *flag),
    }
}

pub fn client_serves_mut<'a>(toggles: &'a mut FeatureToggles, key: &str) -> Option<&'a mut bool> {
    Some(match key {
        "client_serves_self_identity" => &mut toggles.client_serves_self_identity,
        "client_serves_workspace_briefing" => &mut toggles.client_serves_workspace_briefing,
        "client_serves_change_record_tail" => &mut toggles.client_serves_change_record_tail,
        "client_serves_reminders" => &mut toggles.client_serves_reminders,
        "client_serves_recall" => &mut toggles.client_serves_recall,
        "client_serves_agent_lessons" => &mut toggles.client_serves_agent_lessons,
        "client_serves_prompt_shaping" => &mut toggles.client_serves_prompt_shaping,
        "client_serves_proxy_tools" => &mut toggles.client_serves_proxy_tools,
        "client_serves_weasel_guard" => &mut toggles.client_serves_weasel_guard,
        "client_serves_mail" => &mut toggles.client_serves_mail,
        "client_serves_git_read_hint" => &mut toggles.client_serves_git_read_hint,
        "client_serves_memory_keeper" => &mut toggles.client_serves_memory_keeper,
        _ => return None,
    })
}

pub fn set_toggle_enabled(
    toggles: &FeatureToggles,
    key: &str,
    enabled: bool,
) -> Option<FeatureToggles> {
    let mut updated = toggles.clone();
    match key {
        "agent_lessons" => updated.agent_lessons = enabled,
        "change_record" => updated.change_record = enabled,
        "change_record_tail" => updated.change_record_tail = enabled,
        "code_findings" => updated.code_findings = enabled,
        "code_findings_push" => updated.code_findings_push = enabled,
        "code_scan" => updated.code_scan = enabled,
        "channel_link_sweep" => updated.channel_link_sweep = enabled,
        "context_focus_question" => updated.context_focus_question = enabled,
        "db_resync" => updated.db_resync = enabled,
        "dossier_builder" => updated.dossier_builder = enabled,
        "file_read_compression" => updated.file_read_compression = enabled,
        "shell_output_compression" => updated.shell_output_compression = enabled,
        "fleet_auto_start" => updated.fleet_auto_start = enabled,
        "harvester" => updated.harvester = enabled,
        "judge_lessons" => updated.judge_lessons = enabled,
        "judge_worker" => updated.judge_worker = enabled,
        "launch_brief" => updated.launch_brief = enabled,
        "lesson_worker" => updated.lesson_worker = enabled,
        "mailbox_redelivery" => updated.mailbox_redelivery = enabled,
        "memory_keeper" => updated.memory_keeper = enabled,
        "seat_memory_index" => updated.seat_memory_index = enabled,
        "wright" => updated.wright = enabled,
        "mod_lesson_delivery" => updated.mod_lesson_delivery = enabled,
        "native_peer_mail" => updated.native_peer_mail = enabled,
        "prompt_assistant" => updated.prompt_assistant = enabled,
        "proxy_tools" => updated.proxy_tools = enabled,
        "recall_injector" => updated.recall_injector = enabled,
        "reminder_on_token" => updated.reminder_on_token = enabled,
        "reminders" => updated.reminders = enabled,
        "repeated_notices" => updated.repeated_notices = enabled,
        "retention_sweep" => updated.retention_sweep = enabled,
        "codex_diagnostic_capture" => updated.codex_diagnostic_capture = enabled,
        "semantic_context" => updated.semantic_context = enabled,
        "turn_guard" => updated.turn_guard = enabled,
        "directive_proposer" => updated.directive_proposer = enabled,
        "self_identity_reminder" => updated.self_identity_reminder = enabled,
        "session_end_shadow" => updated.session_end_shadow = enabled,
        "session_judge" => updated.session_judge = enabled,
        "steerer" => updated.steerer = enabled,
        "weasel_intercept" => updated.weasel_intercept = enabled,
        "workspace_briefing" => updated.workspace_briefing = enabled,
        "workspace_capture_registration" => updated.workspace_capture_registration = enabled,
        _ => *client_serves_mut(&mut updated, key)? = enabled,
    }
    Some(updated)
}

pub fn build_feature_toggle_overlay(
    config: &crate::config::ProxyConfig,
) -> std::sync::Arc<arc_swap::ArcSwap<FeatureToggles>> {
    let mut toggles = FeatureToggles {
        steerer: config.steerer.enabled,
        ..FeatureToggles::default()
    };
    for (key, enabled) in crate::settings::registry::boot_toggle_values(&config.sections) {
        match set_toggle_enabled(&toggles, key, enabled) {
            Some(updated) => toggles = updated,
            None => tracing::warn!(key, "boot toggle names no feature toggle"),
        }
    }
    std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(toggles))
}
