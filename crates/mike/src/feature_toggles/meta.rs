use serde::{Deserialize, Serialize};

use super::{ToggleApplies, ToggleScope};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToggleMeta {
    pub key: &'static str,
    pub scope: ToggleScope,
    pub applies: ToggleApplies,
}

const fn client_serves(key: &'static str) -> ToggleMeta {
    ToggleMeta {
        key,
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    }
}

pub static TOGGLE_META: &[ToggleMeta] = &[
    ToggleMeta {
        key: "agent_lessons",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "change_record",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "change_record_tail",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "channel_link_sweep",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    client_serves("client_serves_agent_lessons"),
    client_serves("client_serves_change_record_tail"),
    client_serves("client_serves_git_read_hint"),
    client_serves("client_serves_mail"),
    client_serves("client_serves_memory_keeper"),
    client_serves("client_serves_prompt_shaping"),
    client_serves("client_serves_proxy_tools"),
    client_serves("client_serves_recall"),
    client_serves("client_serves_reminders"),
    client_serves("client_serves_self_identity"),
    client_serves("client_serves_weasel_guard"),
    client_serves("client_serves_workspace_briefing"),
    ToggleMeta {
        key: "context_focus_question",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "db_resync",
        scope: ToggleScope::Global,
        applies: ToggleApplies::OnRestart,
    },
    ToggleMeta {
        key: "directive_proposer",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "dossier_builder",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "file_read_compression",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "fleet_auto_start",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "harvester",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "judge_lessons",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "judge_worker",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "launch_brief",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "lesson_worker",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "mailbox_redelivery",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "memory_keeper",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "mod_lesson_delivery",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "native_peer_mail",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "prompt_assistant",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "proxy_tools",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "recall_injector",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "reminder_on_token",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "reminders",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "repeated_notices",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "retention_sweep",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "seat_memory_index",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "semantic_context",
        scope: ToggleScope::Global,
        applies: ToggleApplies::Immediate,
    },
    ToggleMeta {
        key: "self_identity_reminder",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "session_end_shadow",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "session_judge",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "shell_output_compression",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "steerer",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "turn_guard",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
    ToggleMeta {
        key: "weasel_intercept",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "workspace_briefing",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "workspace_capture_registration",
        scope: ToggleScope::Session,
        applies: ToggleApplies::NewSessions,
    },
    ToggleMeta {
        key: "wright",
        scope: ToggleScope::Global,
        applies: ToggleApplies::LiveDisable,
    },
];
