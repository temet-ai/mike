use crate::store::persistence::PersistError;
use crate::store::persistence::consolidation_audit::ConsolidationActionRecord;
use crate::store::persistence::lessons::{LessonRow, SimilarLesson};

use super::session_consolidation::LessonConsolidationCtx;

pub fn runners_up_json(neighbours: &[SimilarLesson]) -> serde_json::Value {
    serde_json::Value::Array(
        neighbours
            .iter()
            .skip(1)
            .map(|n| serde_json::json!({"lesson_id": n.id, "distance": n.distance}))
            .collect(),
    )
}

pub fn skipped_action_record(
    lesson: &LessonRow,
    ctx: &LessonConsolidationCtx,
    error: String,
) -> ConsolidationActionRecord {
    ConsolidationActionRecord {
        lesson_id: lesson.id,
        lesson_text: lesson.text.clone(),
        action_kind: "evaluated",
        outcome: "skipped",
        error: Some(error),
        before_stage: Some("raw"),
        after_stage: None,
        decided_by: None,
        distance: None,
        threshold: Some(ctx.distance_threshold),
        embedding_model: Some(ctx.model_name.clone()),
        counterpart_lesson_id: None,
        counterpart_uuid: None,
        counterpart_text: None,
        runners_up: serde_json::json!([]),
        rank_detail: None,
    }
}

pub fn promoted_action_record(
    lesson: &LessonRow,
    ctx: &LessonConsolidationCtx,
    nearest: Option<&SimilarLesson>,
    runners_up: serde_json::Value,
) -> ConsolidationActionRecord {
    ConsolidationActionRecord {
        lesson_id: lesson.id,
        lesson_text: lesson.text.clone(),
        action_kind: "promoted_to_daily",
        outcome: "applied",
        error: None,
        before_stage: Some("raw"),
        after_stage: Some("daily"),
        decided_by: Some("embedding_distance"),
        distance: nearest.map(|n| n.distance),
        threshold: Some(ctx.distance_threshold),
        embedding_model: Some(ctx.model_name.clone()),
        counterpart_lesson_id: nearest.map(|n| n.id),
        counterpart_uuid: nearest.map(|n| n.uuid),
        counterpart_text: nearest.map(|n| n.text.clone()),
        runners_up,
        rank_detail: None,
    }
}

pub fn promote_failed_action_record(
    lesson: &LessonRow,
    e: &PersistError,
    ctx: &LessonConsolidationCtx,
    nearest: Option<&SimilarLesson>,
    runners_up: serde_json::Value,
) -> ConsolidationActionRecord {
    ConsolidationActionRecord {
        lesson_id: lesson.id,
        lesson_text: lesson.text.clone(),
        action_kind: "promoted_to_daily",
        outcome: "failed",
        error: Some(e.to_string()),
        before_stage: Some("raw"),
        after_stage: None,
        decided_by: Some("embedding_distance"),
        distance: nearest.map(|n| n.distance),
        threshold: Some(ctx.distance_threshold),
        embedding_model: Some(ctx.model_name.clone()),
        counterpart_lesson_id: nearest.map(|n| n.id),
        counterpart_uuid: nearest.map(|n| n.uuid),
        counterpart_text: nearest.map(|n| n.text.clone()),
        runners_up,
        rank_detail: None,
    }
}
