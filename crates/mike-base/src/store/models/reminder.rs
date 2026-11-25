pub const SELF_IDENTITY_MARKER: &str = "proxy-self-identity";
pub const SELF_IDENTITY_ID_LINE: &str = "This Claude Code session's session_id is: ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReminderSource {
    Steerer {
        kind: String,
        intervention_uuid: String,
        advisory: bool,
    },
    Diagnostic,
    Lesson,
    Recall,
    SelfIdentity,
    WorkspaceBriefing,
    SessionMailbox {
        message_uuid: uuid::Uuid,
        from_session_uuid: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct SystemReminder {
    pub source: ReminderSource,
    pub payload: String,
}
