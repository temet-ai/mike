use std::collections::BTreeMap;

use super::state::Candidate;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "probe_session";
pub const QUESTION_SET: &str = "probe_session_v1";

pub fn key(session_id: i64) -> String {
    format!("session_{session_id}")
}

pub fn questions(group: &[&Candidate]) -> BTreeMap<String, Question> {
    group
        .iter()
        .map(|candidate| {
            (
                key(candidate.session_id),
                noul(
                    &format!(
                        "Is the session at sessions.{} a probe: a throwaway test whose only prompt checks that the tooling answers, rather than real work for a person?",
                        key(candidate.session_id)
                    ),
                    "The first prompt is a test, greeting or connectivity check with no real task behind it.",
                    "The first prompt asks for real work, or a question a person would want answered.",
                ),
            )
        })
        .collect()
}
