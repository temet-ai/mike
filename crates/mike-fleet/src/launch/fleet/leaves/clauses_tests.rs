use super::clauses::clauses;

#[test]
fn clauses_of_a_part_list_split_on_semicolons_after_the_label() {
    let text = "builds the forge repos from their specs (syncd T2 part: loose-file conflicts; no .git/ path in the rsync list; no --delete)";
    assert_eq!(
        clauses(text),
        vec![
            "loose-file conflicts",
            "no .git/ path in the rsync list",
            "no --delete"
        ]
    );
}

#[test]
fn clauses_of_a_research_answer_split_its_parts_and_stop_at_the_sentence() {
    let text = "#178 (fleet critic): the seat posts its answer on its issue: what is true now, the options, a recommendation and the measure. Research only; nothing is built or fixed.";
    assert_eq!(
        clauses(text),
        vec![
            "what is true now",
            "the options",
            "a recommendation",
            "the measure"
        ]
    );
}

#[test]
fn clauses_of_a_single_statement_are_none() {
    assert!(clauses("the report is typed").is_empty());
}
