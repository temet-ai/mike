use serde_json::json;

use super::RelationPrompts;
use super::schema::{ADJUDICATOR_SYSTEM_PROMPT, PROPOSER_SYSTEM_PROMPT};
use crate::lessons_settings::meta_memory_loop::{
    LESSON_RELATIONS_ADJUDICATOR_PROMPT_KEY, LESSON_RELATIONS_PROPOSER_PROMPT_KEY,
};

#[test]
fn relation_prompts_without_settings_are_the_built_in_prompts() {
    let prompts = RelationPrompts::from_rows(&[]);
    assert_eq!(prompts.proposer, PROPOSER_SYSTEM_PROMPT);
    assert_eq!(prompts.adjudicator, ADJUDICATOR_SYSTEM_PROMPT);
}

#[test]
fn relation_prompts_with_edited_settings_use_the_edits() {
    let rows = vec![
        (
            LESSON_RELATIONS_PROPOSER_PROMPT_KEY.to_owned(),
            json!("propose carefully"),
        ),
        (
            LESSON_RELATIONS_ADJUDICATOR_PROMPT_KEY.to_owned(),
            json!("check strictly"),
        ),
    ];
    let prompts = RelationPrompts::from_rows(&rows);
    assert_eq!(prompts.proposer, "propose carefully");
    assert_eq!(prompts.adjudicator, "check strictly");
}

#[test]
fn relation_prompts_with_blank_settings_fall_back_to_the_built_in_prompts() {
    let rows = vec![
        (LESSON_RELATIONS_PROPOSER_PROMPT_KEY.to_owned(), json!("")),
        (
            LESSON_RELATIONS_ADJUDICATOR_PROMPT_KEY.to_owned(),
            json!(" "),
        ),
    ];
    assert_eq!(
        RelationPrompts::from_rows(&rows),
        RelationPrompts::from_rows(&[])
    );
}
