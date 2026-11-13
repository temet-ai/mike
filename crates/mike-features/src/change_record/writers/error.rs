use std::path::PathBuf;

use crate::store::persistence::PersistError;

#[derive(Debug, thiserror::Error)]
pub enum WriterError {
    #[error("{} is not inside a git repository", .0.display())]
    NotARepository(PathBuf),
    #[error("could not run `git {command}` in {}: {message}", .checkout.display())]
    Git {
        checkout: PathBuf,
        command: String,
        message: String,
    },
    #[error("could not write the change record: {0}")]
    Persist(#[from] PersistError),
}
