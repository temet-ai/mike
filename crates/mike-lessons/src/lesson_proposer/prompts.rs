use serde_json::Value;

use crate::lessons_settings::meta_memory_loop::{
    LESSON_PLACEMENT_PROMPT_KEY, LESSON_PROPOSER_PROMPT_KEY, LESSON_RELATE_PROMPT_KEY,
    LESSON_SHAPE_PROMPT_KEY, LESSON_TOPIC_PROMPT_KEY, SKILL_AUTHOR_PROMPT_KEY,
    SKILL_REVIEW_PROMPT_KEY, SKILL_TRIGGER_PROMPT_KEY,
};
use crate::settings::memory_prompts::{
    LESSON_PLACEMENT_PROMPT, LESSON_PROPOSER_PROMPT, LESSON_RELATE_PROMPT, LESSON_SHAPE_PROMPT,
    LESSON_TOPIC_PROMPT, SKILL_AUTHOR_PROMPT, SKILL_REVIEW_PROMPT, SKILL_TRIGGER_PROMPT,
    text_from_rows,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposerPrompts {
    pub proposer: String,
    pub relate: String,
    pub placement: String,
    pub shape: String,
    pub topic: String,
    pub skill_author: String,
    pub skill_review: String,
    pub skill_trigger: String,
}

impl Default for ProposerPrompts {
    fn default() -> Self {
        Self::from_rows(&[])
    }
}

impl ProposerPrompts {
    pub fn from_rows(rows: &[(String, Value)]) -> Self {
        let text = |key, default| text_from_rows(rows, key, default);
        Self {
            proposer: text(LESSON_PROPOSER_PROMPT_KEY, LESSON_PROPOSER_PROMPT),
            relate: text(LESSON_RELATE_PROMPT_KEY, LESSON_RELATE_PROMPT),
            placement: text(LESSON_PLACEMENT_PROMPT_KEY, LESSON_PLACEMENT_PROMPT),
            shape: text(LESSON_SHAPE_PROMPT_KEY, LESSON_SHAPE_PROMPT),
            topic: text(LESSON_TOPIC_PROMPT_KEY, LESSON_TOPIC_PROMPT),
            skill_author: text(SKILL_AUTHOR_PROMPT_KEY, SKILL_AUTHOR_PROMPT),
            skill_review: text(SKILL_REVIEW_PROMPT_KEY, SKILL_REVIEW_PROMPT),
            skill_trigger: text(SKILL_TRIGGER_PROMPT_KEY, SKILL_TRIGGER_PROMPT),
        }
    }
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
