pub mod author;
pub mod enabled;
pub mod error;
pub mod events;
pub mod paths;
pub mod words;

#[cfg(test)]
pub mod tests;

pub use author::{Author, author_of};
pub use enabled::change_record_enabled;
pub use error::WriterError;
pub use events::{
    CheckFacts, MailFacts, NoteFacts, record_check, record_mail, record_note, spawn_mail_event,
    spawn_note_event,
};
pub use paths::paths_named;
