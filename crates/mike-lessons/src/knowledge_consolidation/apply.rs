use super::decide::Stmt;
use super::files::{EditRecord, remove_passage, replace_passage};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::consolidation::DecisionApplication;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    Done,
    Proposed,
    Nothing,
    Failed(String),
}

impl Applied {
    pub fn application(&self) -> DecisionApplication {
        match self {
            Self::Done => DecisionApplication::Applied,
            Self::Proposed => DecisionApplication::Proposed,
            Self::Nothing => DecisionApplication::None,
            Self::Failed(_) => DecisionApplication::Failed,
        }
    }

    pub fn error(&self) -> Option<String> {
        match self {
            Self::Failed(message) => Some(message.clone()),
            _ => None,
        }
    }

    pub fn took_effect(&self) -> bool {
        matches!(self, Self::Done | Self::Proposed)
    }
}

pub async fn retire_lesson(
    persist: &PersistHandle,
    stmt: &Stmt,
    reason: &str,
    by: Option<&Stmt>,
) -> Applied {
    let Some(lesson_id) = stmt.lesson_id else {
        return Applied::Failed("the statement is not a lesson".to_owned());
    };
    let retired_by = by.filter(|b| b.is_lesson()).and_then(|b| b.lesson_id);
    match persist
        .lessons()
        .retire_lesson_consolidated(lesson_id, reason, retired_by)
        .await
    {
        Ok(true) => Applied::Done,
        Ok(false) => Applied::Nothing,
        Err(error) => Applied::Failed(error.to_string()),
    }
}

pub async fn merge_lessons(
    persist: &PersistHandle,
    keep: &Stmt,
    others: &[&Stmt],
    text: &str,
) -> Result<(), String> {
    let keep_id = keep.lesson_id.ok_or("the kept statement is not a lesson")?;
    persist
        .lessons()
        .update_lesson_text(keep_id, text)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("the kept lesson was not found or the text is empty")?;
    for other in others {
        let Some(lesson_id) = other.lesson_id else {
            continue;
        };
        let reason = format!("Merged into lesson {keep_id}, which now gives both instructions");
        persist
            .lessons()
            .retire_lesson_consolidated(lesson_id, &reason, Some(keep_id))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn local_paths(
    persist: &PersistHandle,
    machine: Option<Uuid>,
    item_id: i64,
) -> Result<Vec<std::path::PathBuf>, String> {
    let Some(machine) = machine else {
        return Ok(Vec::new());
    };
    let knowledge = persist.knowledge();
    let mut ids = vec![item_id];
    for twin in knowledge
        .twin_carriers(&[item_id])
        .await
        .map_err(|e| e.to_string())?
    {
        ids.push(twin.twin_item_id);
    }
    ids.sort_unstable();
    ids.dedup();
    let locations = knowledge
        .knowledge_file_locations(&ids)
        .await
        .map_err(|e| e.to_string())?;
    Ok(locations
        .into_iter()
        .filter(|l| l.machine_uuid == Some(machine))
        .map(|l| Path::new(&l.root_path).join(&l.relative_path))
        .collect())
}

pub struct FileOutcome {
    pub applied: Applied,
    pub edits: Vec<EditRecord>,
    pub mismatch: bool,
}

impl From<Applied> for FileOutcome {
    fn from(applied: Applied) -> Self {
        Self::plain(applied)
    }
}

impl FileOutcome {
    fn plain(applied: Applied) -> Self {
        Self {
            applied,
            edits: Vec::new(),
            mismatch: false,
        }
    }
}

pub async fn edit_file_statement(
    persist: &PersistHandle,
    machine: Option<Uuid>,
    allow_write: bool,
    stmt: &Stmt,
    replacement: Option<&str>,
) -> FileOutcome {
    if !allow_write {
        return FileOutcome::plain(Applied::Proposed);
    }
    let (Some(passage), Some(start), Some(end)) =
        (&stmt.passage, stmt.passage_start, stmt.passage_end)
    else {
        return FileOutcome::plain(Applied::Failed(
            "the statement has no recorded passage".to_owned(),
        ));
    };
    let paths = match local_paths(persist, machine, stmt.item_id).await {
        Ok(paths) if paths.is_empty() => return FileOutcome::plain(Applied::Proposed),
        Ok(paths) => paths,
        Err(error) => return FileOutcome::plain(Applied::Failed(error)),
    };
    let mut errors = Vec::new();
    let mut edits = Vec::new();
    let mut mismatch = false;
    for path in &paths {
        let span = (start as usize, end as usize);
        let result = match replacement {
            Some(text) => replace_passage(path, span, passage, text),
            None => remove_passage(path, span.0, span.1, passage),
        };
        match result {
            Ok(record) => edits.push(record),
            Err(error) => {
                mismatch |= matches!(error, super::files::EditError::Mismatch(_));
                errors.push(error.to_string());
            }
        }
    }
    let applied = if errors.is_empty() {
        Applied::Done
    } else {
        Applied::Failed(errors.join("; "))
    };
    FileOutcome {
        applied,
        edits,
        mismatch,
    }
}

pub async fn rewrite_lesson(persist: &PersistHandle, stmt: &Stmt, text: &str) -> Applied {
    let Some(lesson_id) = stmt.lesson_id else {
        return Applied::Failed("the statement is not a lesson".to_owned());
    };
    match persist.lessons().update_lesson_text(lesson_id, text).await {
        Ok(Some(_)) => Applied::Done,
        Ok(None) => Applied::Failed("the lesson was not found or is archived".to_owned()),
        Err(error) => Applied::Failed(error.to_string()),
    }
}
