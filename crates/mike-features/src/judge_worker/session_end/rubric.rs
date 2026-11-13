use std::sync::LazyLock;

use crate::judge_worker::session_poll::digest::SESSION_JUDGE_SYSTEM_PROMPT;

pub const RUBRIC_ID: &str = "session-v3-2026-09-24";
pub const RUBRIC_VERSION: i32 = 3;

const SESSION_END_PART: &str = include_str!("rubrics/session-v3-2026-09-24.txt");

static PROMPT: LazyLock<String> =
    LazyLock::new(|| format!("{SESSION_JUDGE_SYSTEM_PROMPT}\n\n{SESSION_END_PART}"));

pub fn system_prompt(rubric_id: &str) -> Option<&'static str> {
    (rubric_id == RUBRIC_ID).then(|| PROMPT.as_str())
}

#[cfg(test)]
pub mod tests {
    use super::{RUBRIC_ID, system_prompt};

    #[test]
    fn the_v3_prompt_is_the_session_prompt_then_the_end_rubric() {
        let p = system_prompt(RUBRIC_ID).unwrap_or_default();
        assert!(p.starts_with("You are a session-quality judge"));
        assert!(p.contains("Default to DONE."));
        assert!(p.contains("CLOSED_IN_FRUSTRATION"));
        assert!(system_prompt("session-v2").is_none());
    }
}
