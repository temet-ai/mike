use chrono::{DateTime, Utc};

use super::dto::{
    AgentDto, LimitsDto, ModelDto, ProfileDto, ProjectDto, SettingBasis, ToggleChangeDto, ToggleDto,
};
use crate::feature_toggles::{FeatureToggles, ResolvedToggles, TOGGLE_META, get_toggle_enabled};
use crate::settings::SettingsOverlay;
use crate::store::persistence::session_delivery::{
    DeliveryAgentRow, DeliveryModelRow, DeliverySessionRow, DeliveryToggleChangeRow,
};

pub(super) const DELIVERY_TOGGLES: [&str; 13] = [
    "reminders",
    "recall_injector",
    "agent_lessons",
    "lesson_worker",
    "self_identity_reminder",
    "workspace_briefing",
    "repeated_notices",
    "proxy_tools",
    "steerer",
    "launch_brief",
    "change_record_tail",
    "reminder_on_token",
    "mailbox_redelivery",
];

pub(super) struct ProfileInputs<'a> {
    pub(super) current_toggles: &'a FeatureToggles,
    pub(super) current_settings: &'a SettingsOverlay,
    pub(super) frozen: Option<&'a ResolvedToggles>,
}

pub(super) struct ProfileRows<'a> {
    pub(super) session: &'a DeliverySessionRow,
    pub(super) models: &'a [DeliveryModelRow],
    pub(super) agents: &'a [DeliveryAgentRow],
    pub(super) toggle_changes: &'a [DeliveryToggleChangeRow],
}

fn frozen_value(frozen: &ResolvedToggles, key: &str) -> Option<bool> {
    match key {
        "reminders" => Some(frozen.reminders),
        "recall_injector" => Some(frozen.recall_injector),
        "agent_lessons" => Some(frozen.agent_lessons),
        "self_identity_reminder" => Some(frozen.self_identity_reminder),
        "workspace_briefing" => Some(frozen.workspace_briefing),
        "repeated_notices" => Some(frozen.repeated_notices),
        "proxy_tools" => Some(frozen.proxy_tools),
        _ => None,
    }
}

fn toggle_dto(
    key: &'static str,
    inputs: &ProfileInputs<'_>,
    changes: &[DeliveryToggleChangeRow],
) -> Option<ToggleDto> {
    let meta = TOGGLE_META.iter().find(|meta| meta.key == key)?;
    let enabled_now = get_toggle_enabled(inputs.current_toggles, key)?;
    let first_change = changes.iter().find(|change| change.toggle_key == key);
    let frozen = inputs.frozen.and_then(|frozen| frozen_value(frozen, key));
    let (enabled, basis) = match (frozen, first_change) {
        (Some(value), _) => (Some(value), SettingBasis::SessionFrozen),
        (None, Some(change)) => (change.enabled_before, SettingBasis::ChangeLogAtSessionStart),
        (None, None) => (Some(enabled_now), SettingBasis::ChangeLogAtSessionStart),
    };
    Some(ToggleDto {
        key,
        scope: meta.scope,
        applies: meta.applies,
        enabled,
        basis,
        enabled_now,
        changed_since_session_start: first_change.is_some(),
    })
}

fn limits(inputs: &ProfileInputs<'_>) -> LimitsDto {
    match inputs.frozen {
        Some(frozen) => LimitsDto {
            inject_lessons: frozen.reminders_inject_lessons,
            max_lessons_to_inject: frozen.reminders_max_lessons_to_inject,
            max_lessons_per_topic: frozen.recall_injector_max_lessons_per_topic,
            basis: SettingBasis::SessionFrozen,
        },
        None => LimitsDto {
            inject_lessons: inputs.current_settings.reminders_inject_lessons,
            max_lessons_to_inject: inputs.current_settings.reminders_max_lessons_to_inject,
            max_lessons_per_topic: inputs
                .current_settings
                .recall_injector_max_lessons_per_topic,
            basis: SettingBasis::CurrentSettings,
        },
    }
}

fn change_dto(change: &DeliveryToggleChangeRow, last_seen: DateTime<Utc>) -> ToggleChangeDto {
    ToggleChangeDto {
        key: change.toggle_key.clone(),
        changed_at: change.changed_at.to_rfc3339(),
        enabled_before: change.enabled_before,
        enabled_after: change.enabled_after,
        via: change.via.clone(),
        during_session: change.changed_at <= last_seen,
    }
}

pub(super) fn build_profile(rows: &ProfileRows<'_>, inputs: &ProfileInputs<'_>) -> ProfileDto {
    let session = rows.session;
    ProfileDto {
        project: ProjectDto {
            id: session.project_id,
            path: session.project_path.clone(),
            name: session.project_name.clone(),
            resolution: session.project_resolution.clone(),
        },
        first_seen: session.first_seen.to_rfc3339(),
        last_seen: session.last_seen.to_rfc3339(),
        models: rows
            .models
            .iter()
            .map(|model| ModelDto {
                model: model.model.clone(),
                request_count: model.request_count,
                requests_with_manifest: model.manifest_count,
            })
            .collect(),
        agents: rows
            .agents
            .iter()
            .map(|agent| AgentDto {
                persona: agent.persona.clone(),
                agent_name: agent.agent_name.clone(),
                first_request_id: agent.first_request_id,
                started_at: agent.started_at.map(|at| at.to_rfc3339()),
            })
            .collect(),
        toggles: DELIVERY_TOGGLES
            .iter()
            .filter_map(|key| toggle_dto(key, inputs, rows.toggle_changes))
            .collect(),
        limits: limits(inputs),
        toggle_changes: rows
            .toggle_changes
            .iter()
            .map(|change| change_dto(change, session.last_seen))
            .collect(),
    }
}
