use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

pub const STATE_OPEN: &str = "open";
pub const STATE_ANSWERED: &str = "answered";
pub const STATE_DECLINED: &str = "declined";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuestionRow {
    pub question_uuid: Uuid,
    pub run_id: Option<String>,
    pub asker: Uuid,
    pub asker_name: Option<String>,
    pub to_session: Option<Uuid>,
    pub to_task: Option<String>,
    pub about_note: Option<Uuid>,
    pub question: String,
    pub blocked: bool,
    pub state: String,
    pub answer: Option<String>,
    pub settled_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
    pub escalated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy)]
pub struct AskedQuestion<'a> {
    pub asker: Uuid,
    pub to_session: Option<Uuid>,
    pub to_task: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub about_note: Option<Uuid>,
    pub question: &'a str,
    pub blocked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleKind {
    Answered,
    Declined,
}

impl SettleKind {
    pub fn state(self) -> &'static str {
        match self {
            Self::Answered => STATE_ANSWERED,
            Self::Declined => STATE_DECLINED,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SettledQuestion<'a> {
    pub question: Uuid,
    pub by: Uuid,
    pub kind: SettleKind,
    pub answer: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettleOutcome {
    Settled(Box<QuestionRow>),
    AlreadySettled(Box<QuestionRow>),
    NotFound,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteRef {
    pub note_uuid: Uuid,
    pub author: Uuid,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub version: Option<i64>,
}
