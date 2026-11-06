use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub session_id: i64,
    pub checkout: String,
    pub in_fleet: bool,
    pub task_files: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AnswerError {
    #[error(transparent)]
    Persist(#[from] PersistError),

    #[error(
        "no commit starting {0:?} is in the record of this checkout; give a longer id or a time"
    )]
    UnknownCommit(String),
}
