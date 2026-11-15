use std::collections::BTreeMap;

use super::state::Candidate;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "probe_session";
pub const QUESTION_SET: &str = "probe_session_v2";

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
                        "Is the session at sessions.{} a probe: a throwaway test whose only prompt checks that the tooling answers, or a scripted test task that exercises one tool with an exact instruction, rather than real work for a person?",
                        key(candidate.session_id)
                    ),
                    "The first prompt is a test, greeting, connectivity check or scripted harness task (an exact instruction to exercise a tool, such as rename a to first and use the Edit tool once) with no real task behind it. No project, a direct origin and one or two turns support this.",
                    "The first prompt asks for real work, or a question a person would want answered. A task in a real project that runs for many turns is real work.",
                ),
            )
        })
        .collect()
}
