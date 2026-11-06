use serde::Serialize;

use crate::feature_toggles::{ToggleApplies, ToggleScope};

#[derive(Debug, Serialize)]
pub(crate) struct SessionDeliveryDto {
    pub(crate) session_uuid: String,
    pub(crate) generated_at: String,
    pub(crate) profile: ProfileDto,
    pub(crate) lessons: Vec<DeliveredLessonDto>,
    pub(crate) topics: Vec<TopicDto>,
    pub(crate) recall_injections: Vec<RecallInjectionDto>,
    pub(crate) skills: Vec<LoadedSkillDto>,
    pub(crate) recall_tool_calls: Vec<RecallToolCallDto>,
    pub(crate) injection_stages: Vec<InjectionStageDto>,
    pub(crate) evidence_gaps: Vec<EvidenceGapDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProfileDto {
    pub(crate) project: ProjectDto,
    pub(crate) first_seen: String,
    pub(crate) last_seen: String,
    pub(crate) models: Vec<ModelDto>,
    pub(crate) agents: Vec<AgentDto>,
    pub(crate) toggles: Vec<ToggleDto>,
    pub(crate) limits: LimitsDto,
    pub(crate) toggle_changes: Vec<ToggleChangeDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectDto {
    pub(crate) id: i64,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) resolution: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModelDto {
    pub(crate) model: Option<String>,
    pub(crate) request_count: i64,
    pub(crate) requests_with_manifest: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentDto {
    pub(crate) persona: Option<String>,
    pub(crate) agent_name: Option<String>,
    pub(crate) first_request_id: Option<i64>,
    pub(crate) started_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SettingBasis {
    SessionFrozen,
    ChangeLogAtSessionStart,
    CurrentSettings,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToggleDto {
    pub(crate) key: &'static str,
    pub(crate) scope: ToggleScope,
    pub(crate) applies: ToggleApplies,
    pub(crate) enabled: Option<bool>,
    pub(crate) basis: SettingBasis,
    pub(crate) enabled_now: bool,
    pub(crate) changed_since_session_start: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct LimitsDto {
    pub(crate) inject_lessons: bool,
    pub(crate) max_lessons_to_inject: usize,
    pub(crate) max_lessons_per_topic: usize,
    pub(crate) basis: SettingBasis,
}

#[derive(Debug, Serialize)]
pub(crate) struct ToggleChangeDto {
    pub(crate) key: String,
    pub(crate) changed_at: String,
    pub(crate) enabled_before: Option<bool>,
    pub(crate) enabled_after: Option<bool>,
    pub(crate) via: String,
    pub(crate) during_session: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct DeliveredLessonDto {
    pub(crate) lesson_id: i64,
    pub(crate) text: String,
    pub(crate) kind: String,
    pub(crate) topic: Option<String>,
    pub(crate) stage: String,
    pub(crate) archived_at: Option<String>,
    pub(crate) archived_reason: Option<String>,
    pub(crate) injection_count: i64,
    pub(crate) first_injected_at: String,
    pub(crate) last_injected_at: String,
    pub(crate) first_request_id: i64,
    pub(crate) last_request_id: i64,
    pub(crate) lanes: Vec<String>,
    pub(crate) personas: Vec<String>,
    pub(crate) origins: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TopicState {
    Recalled,
    DeliveredInFull,
    Available,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopicDto {
    pub(crate) topic: String,
    pub(crate) state: TopicState,
    pub(crate) recall_request_ids: Vec<i64>,
    pub(crate) delivered_lesson_ids: Vec<i64>,
    pub(crate) available_project_lessons: i64,
    pub(crate) available_shared_lessons: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct RecallInjectionDto {
    pub(crate) request_id: i64,
    pub(crate) recalled_at: String,
    pub(crate) topics: Vec<String>,
    pub(crate) lesson_count: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SkillState {
    Loaded,
    LoadFailed,
}

#[derive(Debug, Serialize)]
pub(crate) struct LoadedSkillDto {
    pub(crate) name: Option<String>,
    pub(crate) state: SkillState,
    pub(crate) load_count: i64,
    pub(crate) error_count: i64,
    pub(crate) first_request_id: i64,
    pub(crate) last_request_id: i64,
    pub(crate) first_loaded_at: String,
    pub(crate) last_loaded_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RecallToolCallDto {
    pub(crate) tool_name: String,
    pub(crate) call_count: i64,
    pub(crate) error_count: i64,
    pub(crate) first_request_id: i64,
    pub(crate) last_request_id: i64,
    pub(crate) first_called_at: String,
    pub(crate) last_called_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct InjectionStageDto {
    pub(crate) stage: String,
    pub(crate) outcome: String,
    pub(crate) reason_code: String,
    pub(crate) request_count: i64,
    pub(crate) first_request_id: i64,
    pub(crate) last_request_id: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvidenceGapDto {
    pub(crate) code: &'static str,
    pub(crate) detail: String,
}
