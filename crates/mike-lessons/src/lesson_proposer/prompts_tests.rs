use serde_json::json;

use super::ProposerPrompts;
use crate::lessons_settings::meta_memory_loop::{
    LESSON_PLACEMENT_PROMPT_KEY, LESSON_PROPOSER_PROMPT_KEY, LESSON_RELATE_PROMPT_KEY,
    LESSON_SHAPE_PROMPT_KEY, LESSON_TOPIC_PROMPT_KEY, SKILL_AUTHOR_PROMPT_KEY,
    SKILL_REVIEW_PROMPT_KEY, SKILL_TRIGGER_PROMPT_KEY,
};
use crate::settings::memory_prompts::{
    LESSON_PLACEMENT_PROMPT, LESSON_PROPOSER_PROMPT, LESSON_RELATE_PROMPT, LESSON_SHAPE_PROMPT,
    LESSON_TOPIC_PROMPT, SKILL_AUTHOR_PROMPT, SKILL_REVIEW_PROMPT, SKILL_TRIGGER_PROMPT,
};

const KEYS: [&str; 8] = [
    LESSON_PROPOSER_PROMPT_KEY,
    LESSON_RELATE_PROMPT_KEY,
    LESSON_PLACEMENT_PROMPT_KEY,
    LESSON_SHAPE_PROMPT_KEY,
    LESSON_TOPIC_PROMPT_KEY,
    SKILL_AUTHOR_PROMPT_KEY,
    SKILL_REVIEW_PROMPT_KEY,
    SKILL_TRIGGER_PROMPT_KEY,
];

fn texts(prompts: &ProposerPrompts) -> [&str; 8] {
    [
        &prompts.proposer,
        &prompts.relate,
        &prompts.placement,
        &prompts.shape,
        &prompts.topic,
        &prompts.skill_author,
        &prompts.skill_review,
        &prompts.skill_trigger,
    ]
}

const BUILT_IN: [&str; 8] = [
    LESSON_PROPOSER_PROMPT,
    LESSON_RELATE_PROMPT,
    LESSON_PLACEMENT_PROMPT,
    LESSON_SHAPE_PROMPT,
    LESSON_TOPIC_PROMPT,
    SKILL_AUTHOR_PROMPT,
    SKILL_REVIEW_PROMPT,
    SKILL_TRIGGER_PROMPT,
];

#[test]
fn proposer_prompts_without_settings_are_the_built_in_prompts() {
    assert_eq!(texts(&ProposerPrompts::from_rows(&[])), BUILT_IN);
}

#[test]
fn proposer_prompts_with_edited_settings_use_each_edit() {
    let rows: Vec<_> = KEYS
        .iter()
        .map(|key| ((*key).to_owned(), json!(format!("edited {key}"))))
        .collect();
    let prompts = ProposerPrompts::from_rows(&rows);
    for (key, text) in KEYS.iter().zip(texts(&prompts)) {
        assert_eq!(text, format!("edited {key}"));
    }
}

#[test]
fn proposer_prompts_with_blank_settings_fall_back_to_the_built_in_prompts() {
    let rows: Vec<_> = KEYS
        .iter()
        .map(|key| ((*key).to_owned(), json!("  \n")))
        .collect();
    assert_eq!(texts(&ProposerPrompts::from_rows(&rows)), BUILT_IN);
}
