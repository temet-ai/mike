use serde_json::{Value, json};

use crate::llm::decisions::text::head_chars;

const MESSAGE_CHARS: usize = 1_200;
const DIFF_CHARS: usize = 7_000;
const FILES_SHOWN: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub message: String,
    pub files: Vec<String>,
    pub diff: String,
}

pub fn state(commit: &Commit) -> Value {
    json!({
        "commit_message": head_chars(&commit.message, MESSAGE_CHARS),
        "files_changed": commit.files.iter().take(FILES_SHOWN).collect::<Vec<_>>(),
        "files_changed_total": commit.files.len(),
        "diff": head_chars(&commit.diff, DIFF_CHARS),
    })
}
