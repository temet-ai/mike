pub const RETIRED_LABEL_SOURCES: [(&str, &str); 15] = [
    ("change_author", "next_commit_author"),
    ("weasel_after", "jev_after_stream"),
    ("diagnostic_phrase", "next_turn_reaction"),
    ("text_origin", "next_turn_reaction"),
    ("directive", "next_turn_reaction"),
    ("step_class", "next_turn_reaction"),
    ("ended_on", "next_turn_reaction"),
    ("prod_detect", "next_turn_reaction"),
    ("steerer_checklist", "next_turn_reaction"),
    ("steerer_checklist", "steerer_llm_verdict"),
    ("verifier_precheck", "turn_guard_label"),
    ("turn_guard_precheck", "turn_guard_label"),
    ("turn_judge", "operator_label"),
    ("commit_message", "next_turn_reaction_after_note"),
    ("rule_check", "next_turn_reaction_after_note"),
];

pub fn retired_columns() -> (Vec<String>, Vec<String>) {
    RETIRED_LABEL_SOURCES
        .iter()
        .map(|(point, source)| ((*point).to_owned(), (*source).to_owned()))
        .unzip()
}
