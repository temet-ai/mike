use chrono::{DateTime, Utc};

pub mod reads;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionNoteRow {
    pub note_uuid: uuid::Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SessionNoteWithAuthor {
    pub note_uuid: uuid::Uuid,
    pub session_uuid: uuid::Uuid,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct NewSessionNote<'a> {
    pub kind: Option<&'a str>,
    pub title: &'a str,
    pub body: Option<&'a str>,
    pub status: Option<&'a str>,
    pub tags: Option<&'a [String]>,
    pub data: Option<&'a serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SessionNoteUpdate<'a> {
    pub kind: Option<&'a str>,
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub status: Option<&'a str>,
    pub tags: Option<&'a [String]>,
    pub data: Option<&'a serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteOrder {
    #[default]
    NewestFirst,
    OldestFirst,
}

impl NoteOrder {
    pub fn sql_direction(self) -> &'static str {
        match self {
            NoteOrder::NewestFirst => "DESC",
            NoteOrder::OldestFirst => "ASC",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SessionNoteFilter<'a> {
    pub kind: Option<&'a str>,
    pub status: Option<&'a str>,
    pub tag: Option<&'a str>,
    pub q: Option<&'a str>,
    pub order: NoteOrder,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectNoteFilter<'a> {
    pub role: Option<&'a str>,
    pub name: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub status: Option<&'a str>,
    pub tag: Option<&'a str>,
    pub session_uuid: Option<uuid::Uuid>,
    pub q: Option<&'a str>,
    pub order: NoteOrder,
    pub limit: i64,
    pub offset: i64,
}
