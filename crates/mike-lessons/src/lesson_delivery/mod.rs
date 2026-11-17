pub mod attribution;
#[cfg(test)]
mod earlier_tests;
pub mod held;
pub mod holdings;
pub mod hooks;
pub mod origin;
pub mod select;
pub mod supplied;

use sha2::{Digest, Sha256};

pub use supplied::remember_scores;

pub use held::{HeldTag, REVISION_MARK, held_lesson_ids, held_lesson_tags, is_held};
pub use origin::label_mod_lessons;
pub use select::catalog_for_prompt;
pub use select::{
    DeliveredLesson, PromptLessons, already_present_ids, client_held, revisions_of,
    select_for_prompt, tagged_in_body,
};

pub const LESSON_TAG_OPEN: &str = "[lesson:";
pub const LESSONS_HEADING: &str = "Lessons recorded from earlier sessions in this project:";
pub const CLIENT_ROUTE: &str = "mod";

pub fn bucket(salt: &str, id: &str) -> u16 {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(id.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    u16::from_be_bytes([digest[0], digest[1]]) % 100
}

pub fn client_holds_lessons(
    toggle_on: bool,
    snapshot: &crate::lesson_worker::LessonSnapshot,
    session_id: &str,
) -> bool {
    toggle_on && snapshot.relevance.holdings().is_client(session_id)
}

#[cfg(test)]
pub mod origin_tests;
#[cfg(test)]
mod tests;
