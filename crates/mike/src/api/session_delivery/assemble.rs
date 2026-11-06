use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::dto::{
    DeliveredLessonDto, EvidenceGapDto, InjectionStageDto, LoadedSkillDto, RecallInjectionDto,
    RecallToolCallDto, SessionDeliveryDto, SkillState, TopicDto, TopicState,
};
use super::profile::{ProfileInputs, ProfileRows, build_profile};
use crate::store::persistence::session_delivery::{
    AvailableTopicRow, DeliveredLessonRow, DeliveryRecallRow, DeliveryStageRow, DeliveryToolUseRow,
    SessionDeliveryRows,
};

const SKILL_TOOL: &str = "Skill";
const LESSON_STAGES: [&str; 2] = ["session_lessons", "agent_lessons"];

pub(super) fn assemble(
    rows: &SessionDeliveryRows,
    inputs: &ProfileInputs<'_>,
    generated_at: DateTime<Utc>,
) -> SessionDeliveryDto {
    let profile_rows = ProfileRows {
        session: &rows.session,
        models: &rows.models,
        agents: &rows.agents,
        toggle_changes: &rows.toggle_changes,
    };
    let (skills, recall_tool_calls) = split_tool_uses(&rows.tool_uses);
    SessionDeliveryDto {
        session_uuid: rows.session.session_uuid.to_string(),
        generated_at: generated_at.to_rfc3339(),
        profile: build_profile(&profile_rows, inputs),
        lessons: rows.lessons.iter().map(lesson_dto).collect(),
        topics: topics(&rows.available_topics, &rows.recalls, &rows.lessons),
        recall_injections: rows.recalls.iter().map(recall_dto).collect(),
        skills,
        recall_tool_calls,
        injection_stages: rows.stages.iter().map(stage_dto).collect(),
        evidence_gaps: evidence_gaps(rows, inputs.frozen.is_some()),
    }
}

fn lesson_dto(row: &DeliveredLessonRow) -> DeliveredLessonDto {
    DeliveredLessonDto {
        lesson_id: row.lesson_id,
        text: row.text.clone(),
        kind: row.kind.clone(),
        topic: row.topic.clone(),
        stage: row.stage.clone(),
        archived_at: row.archived_at.map(|at| at.to_rfc3339()),
        archived_reason: row.archived_reason.clone(),
        injection_count: row.injection_count,
        first_injected_at: row.first_injected_at.to_rfc3339(),
        last_injected_at: row.last_injected_at.to_rfc3339(),
        first_request_id: row.first_request_id,
        last_request_id: row.last_request_id,
        lanes: row.lanes.clone(),
        personas: row.personas.clone(),
        origins: row.origins.clone(),
    }
}

fn recall_topics(row: &DeliveryRecallRow) -> Vec<String> {
    row.topics
        .as_array()
        .map(|topics| {
            topics
                .iter()
                .filter_map(|topic| topic.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn recall_dto(row: &DeliveryRecallRow) -> RecallInjectionDto {
    RecallInjectionDto {
        request_id: row.request_id,
        recalled_at: row.recalled_at.to_rfc3339(),
        topics: recall_topics(row),
        lesson_count: row.lesson_count,
    }
}

#[derive(Default)]
struct TopicAccumulator {
    recall_request_ids: Vec<i64>,
    delivered_lesson_ids: Vec<i64>,
    project_lessons: i64,
    shared_lessons: i64,
}

fn topics(
    available: &[AvailableTopicRow],
    recalls: &[DeliveryRecallRow],
    lessons: &[DeliveredLessonRow],
) -> Vec<TopicDto> {
    let mut by_topic: BTreeMap<String, TopicAccumulator> = BTreeMap::new();
    for row in available {
        let entry = by_topic.entry(row.topic.clone()).or_default();
        if row.shared {
            entry.shared_lessons += row.lesson_count;
        } else {
            entry.project_lessons += row.lesson_count;
        }
    }
    for recall in recalls {
        for topic in recall_topics(recall) {
            by_topic
                .entry(topic)
                .or_default()
                .recall_request_ids
                .push(recall.request_id);
        }
    }
    for lesson in lessons {
        if let Some(topic) = &lesson.topic {
            by_topic
                .entry(topic.clone())
                .or_default()
                .delivered_lesson_ids
                .push(lesson.lesson_id);
        }
    }
    by_topic
        .into_iter()
        .map(|(topic, acc)| TopicDto {
            state: topic_state(&acc),
            topic,
            recall_request_ids: acc.recall_request_ids,
            delivered_lesson_ids: acc.delivered_lesson_ids,
            available_project_lessons: acc.project_lessons,
            available_shared_lessons: acc.shared_lessons,
        })
        .collect()
}

fn topic_state(acc: &TopicAccumulator) -> TopicState {
    if !acc.recall_request_ids.is_empty() {
        TopicState::Recalled
    } else if !acc.delivered_lesson_ids.is_empty() {
        TopicState::DeliveredInFull
    } else {
        TopicState::Available
    }
}

fn split_tool_uses(rows: &[DeliveryToolUseRow]) -> (Vec<LoadedSkillDto>, Vec<RecallToolCallDto>) {
    let mut skills = Vec::new();
    let mut calls = Vec::new();
    for row in rows {
        if row.tool_name == SKILL_TOOL {
            skills.push(LoadedSkillDto {
                name: row.skill.clone(),
                state: if row.use_count > row.error_count {
                    SkillState::Loaded
                } else {
                    SkillState::LoadFailed
                },
                load_count: row.use_count,
                error_count: row.error_count,
                first_request_id: row.first_request_id,
                last_request_id: row.last_request_id,
                first_loaded_at: row.first_used_at.to_rfc3339(),
                last_loaded_at: row.last_used_at.to_rfc3339(),
            });
        } else {
            calls.push(RecallToolCallDto {
                tool_name: row.tool_name.clone(),
                call_count: row.use_count,
                error_count: row.error_count,
                first_request_id: row.first_request_id,
                last_request_id: row.last_request_id,
                first_called_at: row.first_used_at.to_rfc3339(),
                last_called_at: row.last_used_at.to_rfc3339(),
            });
        }
    }
    (skills, calls)
}

fn stage_dto(row: &DeliveryStageRow) -> InjectionStageDto {
    InjectionStageDto {
        stage: row.stage.clone(),
        outcome: row.outcome.clone(),
        reason_code: row.reason_code.clone(),
        request_count: row.request_count,
        first_request_id: row.first_request_id,
        last_request_id: row.last_request_id,
    }
}

fn applied_requests(stages: &[DeliveryStageRow], names: &[&str]) -> i64 {
    stages
        .iter()
        .filter(|row| row.outcome == "applied" && names.contains(&row.stage.as_str()))
        .map(|row| row.request_count)
        .sum()
}

fn evidence_gaps(rows: &SessionDeliveryRows, frozen_present: bool) -> Vec<EvidenceGapDto> {
    let mut gaps = Vec::new();
    let request_count: i64 = rows.models.iter().map(|model| model.request_count).sum();
    let manifest_count: i64 = rows.models.iter().map(|model| model.manifest_count).sum();
    if manifest_count < request_count {
        gaps.push(EvidenceGapDto {
            code: "requests_without_manifest",
            detail: format!(
                "{} of {request_count} requests carry no transformation manifest, so stage evidence covers only the other {manifest_count}.",
                request_count - manifest_count
            ),
        });
    }
    let lesson_reminders = applied_requests(&rows.stages, &LESSON_STAGES);
    if lesson_reminders > rows.attributed_lesson_requests {
        gaps.push(EvidenceGapDto {
            code: "lesson_reminders_unattributed",
            detail: format!(
                "A lesson reminder was applied {lesson_reminders} times but only {} requests have lessons attributed. The rest were topic-catalog-only reminders, or their lessons were deleted afterwards.",
                rows.attributed_lesson_requests
            ),
        });
    }
    if lesson_reminders > 0 {
        gaps.push(EvidenceGapDto {
            code: "topic_catalog_not_recorded",
            detail: "Which topic labels a lesson reminder offered is not stored per topic. A topic counts as recalled only when the manifest shows its lessons were injected on request.".to_owned(),
        });
    }
    if !rows.lessons.is_empty() {
        gaps.push(EvidenceGapDto {
            code: "lesson_text_not_versioned",
            detail: "Lesson text shown here is the lesson as it is stored now. Lessons keep no revision history, so an edit made after delivery is not visible.".to_owned(),
        });
    }
    gaps.push(EvidenceGapDto {
        code: "skill_selection_not_recorded",
        detail: "Mike does not record which skills were offered to the session or which package version was loaded. Only skills the session itself loaded are listed.".to_owned(),
    });
    if !frozen_present {
        gaps.push(EvidenceGapDto {
            code: "session_settings_not_in_memory",
            detail: "The proxy no longer holds this session's frozen settings, so toggle values come from the change log relative to the session's first request.".to_owned(),
        });
    }
    gaps
}
