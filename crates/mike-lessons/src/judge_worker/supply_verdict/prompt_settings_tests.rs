use serde_json::json;

use super::{SYSTEM_PROMPT, prompt_from_rows};
use crate::lessons_settings::meta_memory_loop::SUPPLY_VERDICT_PROMPT_KEY;

#[test]
fn prompt_from_rows_without_the_setting_is_the_built_in_prompt() {
    assert_eq!(prompt_from_rows(&[]), SYSTEM_PROMPT);
}

#[test]
fn prompt_from_rows_with_an_edited_prompt_uses_it() {
    let rows = vec![(
        SUPPLY_VERDICT_PROMPT_KEY.to_owned(),
        json!("judge it strictly"),
    )];
    assert_eq!(prompt_from_rows(&rows), "judge it strictly");
}

#[test]
fn prompt_from_rows_with_a_blank_prompt_falls_back_to_the_built_in_prompt() {
    let rows = vec![(SUPPLY_VERDICT_PROMPT_KEY.to_owned(), json!("  \n "))];
    assert_eq!(prompt_from_rows(&rows), SYSTEM_PROMPT);
}
