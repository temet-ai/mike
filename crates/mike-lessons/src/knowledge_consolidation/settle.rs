use super::apply::{Applied, FileOutcome, edit_file_statement, retire_lesson};
use super::context::stmt_of;
use super::decide::Stmt;
use super::execute::attach_edits;
use super::record::{Entry, Recorder, text_before};
use super::settings::Settings;
use super::undo::DecisionError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::consolidation::{DecisionRow, StatementStateChange};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

fn failed(error: impl std::fmt::Display) -> DecisionError {
    DecisionError::Failed(error.to_string())
}

fn conflict(message: &str) -> DecisionError {
    DecisionError::Conflict(message.to_owned())
}

pub struct Settled {
    pub decision: DecisionRow,
    pub conflict: DecisionRow,
}

async fn current_pair(
    persist: &PersistHandle,
    decision: &DecisionRow,
    keep_id: i64,
) -> Result<(Stmt, Stmt), DecisionError> {
    let (Some(a), Some(b)) = (decision.statement_id, decision.counterpart_statement_id) else {
        return Err(conflict("a statement of this conflict no longer exists"));
    };
    if keep_id != a && keep_id != b {
        return Err(conflict(
            "keep_statement_id must be one of the two statements in this conflict",
        ));
    }
    let details = persist
        .knowledge()
        .statement_details(&[a, b])
        .await
        .map_err(failed)?;
    if details.len() != 2 || details.iter().any(|d| d.state != "current") {
        return Err(conflict("this conflict was already settled or one of its statements is gone"));
    }
    let mut stmts: Vec<Stmt> = details.into_iter().map(stmt_of).collect();
    let keep_at = stmts.iter().position(|s| s.id == keep_id);
    let Some(keep_at) = keep_at else {
        return Err(conflict("the kept statement was not found"));
    };
    let keep = stmts.remove(keep_at);
    Ok((keep, stmts.remove(0)))
}

pub async fn settle_conflict(
    persist: &PersistHandle,
    machine: Option<Uuid>,
    decision_id: i64,
    keep_statement_id: i64,
) -> Result<Settled, DecisionError> {
    let knowledge = persist.knowledge();
    let original = knowledge
        .get_consolidation_decision(decision_id)
        .await
        .map_err(failed)?
        .ok_or(DecisionError::NotFound)?;
    if original.verdict != "conflict_open" {
        return Err(conflict("only an open conflict can be settled"));
    }
    let (keep, loser) = current_pair(persist, &original, keep_statement_id).await?;
    let settings = Settings::load(persist).await;
    let reason = format!("Kept by the operator over {}", keep.carrier);
    let outcome = if loser.is_lesson() {
        FileOutcome::from(retire_lesson(persist, &loser, &reason, Some(&keep)).await)
    } else {
        edit_file_statement(persist, machine, settings.edit_files, &loser, None).await
    };
    let run_id = persist
        .lessons()
        .start_consolidation_run("knowledge", "manual")
        .await
        .map_err(failed)?;
    let recorder = Recorder {
        run_id,
        project_id: original.project_id,
        subject_id: original.subject_id.unwrap_or_default(),
        model: "operator".to_owned(),
        evidence: json!([{
            "reference": "operator", "kind": "correction", "similarity": 1.0,
            "excerpt": "The operator chose which of the two statements to keep",
            "dated_at": Utc::now(), "used": true,
            "shows": "The operator chose which of the two statements to keep"
        }]),
    };
    let applied = outcome.applied.clone();
    let mut new = recorder.decision(
        &loser,
        Entry {
            verdict: "superseded",
            counterpart: Some(&keep),
            before: text_before(&loser),
            after: loser.is_file().then(String::new),
            reason,
            applied: applied.clone(),
        },
    );
    new.subject_id = original.subject_id;
    attach_edits(&mut new, &outcome.edits);
    if !matches!(applied, Applied::Failed(_)) {
        knowledge
            .set_statement_states(&[StatementStateChange {
                statement_id: loser.id,
                state: "superseded".to_owned(),
                resolved_by_statement_id: Some(keep.id),
            }])
            .await
            .map_err(failed)?;
    }
    let ids = knowledge
        .write_consolidation_decisions(&[new])
        .await
        .map_err(failed)?;
    persist
        .lessons()
        .finish_consolidation_run(run_id, "completed", None)
        .await
        .map_err(failed)?;
    let new_id = ids.first().copied().ok_or_else(|| failed("no decision was written"))?;
    let decision = knowledge
        .get_consolidation_decision(new_id)
        .await
        .map_err(failed)?
        .ok_or(DecisionError::NotFound)?;
    let conflict_row = knowledge
        .get_consolidation_decision(decision_id)
        .await
        .map_err(failed)?
        .ok_or(DecisionError::NotFound)?;
    Ok(Settled {
        decision,
        conflict: conflict_row,
    })
}
