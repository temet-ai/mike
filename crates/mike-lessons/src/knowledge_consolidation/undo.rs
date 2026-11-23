use super::apply::{Applied, edit_file_statement};
use super::execute::edit_items;
use super::files::{EditRecord, reinsert};
use super::context::stmt_of;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::consolidation::{
    DecisionApplication, DecisionRow, StatementStateChange,
};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionError {
    NotFound,
    Conflict(String),
    Failed(String),
}

fn failed(error: impl std::fmt::Display) -> DecisionError {
    DecisionError::Failed(error.to_string())
}

async fn load(persist: &PersistHandle, id: i64) -> Result<DecisionRow, DecisionError> {
    persist
        .knowledge()
        .get_consolidation_decision(id)
        .await
        .map_err(failed)?
        .ok_or(DecisionError::NotFound)
}

async fn reload(persist: &PersistHandle, id: i64) -> Result<DecisionRow, DecisionError> {
    load(persist, id).await
}

pub async fn undo_decision(persist: &PersistHandle, id: i64) -> Result<DecisionRow, DecisionError> {
    let decision = load(persist, id).await?;
    if decision.application != "applied" {
        return Err(DecisionError::Conflict(format!(
            "decision {id} is {}, and only an applied decision can be undone",
            decision.application
        )));
    }
    let item_id = decision.item_id.ok_or_else(|| {
        DecisionError::Conflict("the item this decision changed no longer exists".to_owned())
    })?;
    let knowledge = persist.knowledge();
    let lesson_id = knowledge.lesson_id_of_item(item_id).await.map_err(failed)?;
    let Some(lesson_id) = lesson_id else {
        return undo_file_decision(persist, &decision).await;
    };
    if matches!(decision.verdict.as_str(), "tightened" | "corrected") {
        persist
            .lessons()
            .update_lesson_text(lesson_id, &decision.text_before)
            .await
            .map_err(failed)?
            .ok_or_else(|| DecisionError::Conflict("the lesson no longer exists".to_owned()))?;
    } else if !persist
        .lessons()
        .restore_consolidated_lesson(lesson_id)
        .await
        .map_err(failed)?
    {
        return Err(DecisionError::Conflict(
            "the lesson is not retired as consolidated any more".to_owned(),
        ));
    }
    if let Some(statement_id) = decision.statement_id
        && matches!(decision.verdict.as_str(), "covered" | "superseded" | "merged")
    {
        knowledge
            .set_statement_states(&[StatementStateChange {
                statement_id,
                state: "current".to_owned(),
                resolved_by_statement_id: None,
            }])
            .await
            .map_err(failed)?;
    }
    knowledge
        .set_decision_application(id, DecisionApplication::Undone, None)
        .await
        .map_err(failed)?;
    reload(persist, id).await
}

fn recorded_edits(decision: &DecisionRow) -> Vec<EditRecord> {
    decision
        .evidence
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("file_edit"))
        .filter_map(|edit| serde_json::from_value(edit.clone()).ok())
        .collect()
}

async fn undo_file_decision(
    persist: &PersistHandle,
    decision: &DecisionRow,
) -> Result<DecisionRow, DecisionError> {
    let edits = recorded_edits(decision);
    if edits.is_empty() {
        return Err(DecisionError::Conflict(format!(
            "decision {} edited a file but no edit was recorded; the passage it removed is in text_before",
            decision.id
        )));
    }
    let mut errors = Vec::new();
    let mut restored = 0;
    for edit in &edits {
        match reinsert(edit) {
            Ok(()) => restored += 1,
            Err(error) => errors.push(error.to_string()),
        }
    }
    if restored == 0 {
        return Err(DecisionError::Conflict(errors.join("; ")));
    }
    let knowledge = persist.knowledge();
    if let Some(statement_id) = decision.statement_id {
        knowledge
            .set_statement_states(&[StatementStateChange {
                statement_id,
                state: "current".to_owned(),
                resolved_by_statement_id: None,
            }])
            .await
            .map_err(failed)?;
    }
    knowledge
        .set_decision_application(decision.id, DecisionApplication::Undone, None)
        .await
        .map_err(failed)?;
    reload(persist, decision.id).await
}

pub async fn apply_decision(
    persist: &PersistHandle,
    machine: Option<Uuid>,
    id: i64,
) -> Result<DecisionRow, DecisionError> {
    let decision = load(persist, id).await?;
    if !matches!(decision.application.as_str(), "proposed" | "failed") {
        return Err(DecisionError::Conflict(format!(
            "decision {id} is {}, and only a proposed or failed edit can be applied",
            decision.application
        )));
    }
    let statement_id = decision.statement_id.ok_or_else(|| {
        DecisionError::Conflict("the statement this decision names no longer exists".to_owned())
    })?;
    let knowledge = persist.knowledge();
    let detail = knowledge
        .statement_details(&[statement_id])
        .await
        .map_err(failed)?
        .into_iter()
        .next()
        .ok_or_else(|| DecisionError::Conflict("the statement no longer exists".to_owned()))?;
    if detail.state == "gone" {
        return Err(DecisionError::Conflict(
            "the file was split again since this was proposed, so the recorded place is stale"
                .to_owned(),
        ));
    }
    let subject_id = detail.subject_id;
    let replacement = matches!(decision.verdict.as_str(), "corrected" | "tightened")
        .then(|| decision.text_after.clone().unwrap_or_default());
    let outcome = edit_file_statement(persist, machine, true, &stmt_of(detail), replacement.as_deref()).await;
    record_edits(persist, id, &outcome.edits).await?;
    match outcome.applied {
        Applied::Done => {
            knowledge
                .set_decision_application(id, DecisionApplication::Applied, None)
                .await
                .map_err(failed)?;
            reload(persist, id).await
        }
        Applied::Proposed | Applied::Nothing => Err(DecisionError::Conflict(
            "no checkout of this file exists on this machine".to_owned(),
        )),
        Applied::Failed(message) => {
            knowledge
                .set_decision_application(id, DecisionApplication::Failed, Some(&message))
                .await
                .map_err(failed)?;
            if let (true, Some(subject)) = (outcome.mismatch, subject_id) {
                knowledge
                    .mark_knowledge_subjects_changed(&[subject], chrono::Utc::now())
                    .await
                    .map_err(failed)?;
            }
            Err(DecisionError::Conflict(message))
        }
    }
}

async fn record_edits(
    persist: &PersistHandle,
    id: i64,
    edits: &[EditRecord],
) -> Result<(), DecisionError> {
    if edits.is_empty() {
        return Ok(());
    }
    persist
        .knowledge()
        .append_decision_evidence(id, &serde_json::Value::Array(edit_items(edits)))
        .await
        .map_err(failed)?;
    Ok(())
}

const STALE_NOTE: &str = "the file changed before the edit was made";

fn machine_owned(detail: &crate::store::persistence::consolidation::StatementDetail) -> bool {
    detail.source_name.as_deref() == Some("claude-memory") || detail.scope.as_deref() == Some("machine")
}

async fn close_if_stale(persist: &PersistHandle, row: &DecisionRow) -> Option<bool> {
    let knowledge = persist.knowledge();
    let detail = match row.statement_id {
        None => None,
        Some(id) => knowledge.statement_details(&[id]).await.ok()?.into_iter().next(),
    };
    match detail {
        Some(d) if d.state != "gone" => Some(machine_owned(&d)),
        _ => {
            knowledge.close_stale_decision(row.id, STALE_NOTE).await.ok()?;
            None
        }
    }
}

async fn list_proposed(persist: &PersistHandle, machine: Uuid, limit: i64) -> Vec<i64> {
    let listed = persist.knowledge().proposed_decisions_on_machine(machine, limit).await;
    listed.unwrap_or_else(|error| {
        tracing::warn!(%error, "knowledge consolidation: could not list proposed file edits");
        Vec::new()
    })
}

async fn apply_if_ours(persist: &PersistHandle, machine: Uuid, id: i64) -> bool {
    let Ok(Some(row)) = persist.knowledge().get_consolidation_decision(id).await else {
        return false;
    };
    close_if_stale(persist, &row).await == Some(true)
        && apply_decision(persist, Some(machine), id).await.is_ok()
}

pub async fn apply_proposed_on_machine(persist: &PersistHandle, machine: Uuid, limit: i64) -> u32 {
    let mut applied = 0;
    for id in list_proposed(persist, machine, limit).await {
        applied += u32::from(apply_if_ours(persist, machine, id).await);
    }
    applied
}
