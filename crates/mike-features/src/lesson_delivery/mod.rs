pub mod held;
pub mod origin;
pub mod select;
pub mod supplied;

use sha2::{Digest, Sha256};

pub use supplied::remember_scores;

pub use held::held_lesson_ids;
pub use origin::label_mod_lessons;
pub use select::catalog_for_prompt;
pub use select::{DeliveredLesson, PromptLessons, select_for_prompt};

pub const LESSON_TAG_OPEN: &str = "[lesson:";
pub const LESSONS_HEADING: &str = "Lessons recorded from earlier sessions in this project:";
pub const CONTROL_PERCENT: u16 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    Mod,
    Control,
}

impl Arm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Control => "control",
        }
    }
}

pub fn bucket(salt: &str, id: &str) -> u16 {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(id.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    u16::from_be_bytes([digest[0], digest[1]]) % 100
}

pub fn arm_for(session_id: &str) -> Arm {
    if bucket("", session_id) < CONTROL_PERCENT {
        Arm::Control
    } else {
        Arm::Mod
    }
}

pub fn client_holds_lessons(
    toggle_on: bool,
    snapshot: &crate::lesson_worker::LessonSnapshot,
    session_id: &str,
    body: &[u8],
) -> bool {
    toggle_on
        && (snapshot.relevance.is_mod_session(session_id) || !held_lesson_ids(body).is_empty())
}

#[cfg(test)]
pub mod origin_tests;
#[cfg(test)]
mod tests;
