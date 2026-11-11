use super::quoted::{
    SHOWN_CHARS, SHOWN_PER_EVENT, ShownText, WindowTurn, quoted_references, words,
};

const PASSAGE: &str = "The reranker passages exceed budget because candidates arrive unsorted \
                       during warmup, so the first request waits for the whole batch.";

fn shown(reference: &str, text: &str) -> ShownText {
    ShownText {
        reference: reference.to_owned(),
        text: text.to_owned(),
    }
}

fn turn(answer: &str, prompt: &str) -> WindowTurn {
    WindowTurn {
        answer: answer.to_owned(),
        prompt: prompt.to_owned(),
    }
}

fn quoted(passage: &str, turns: &[WindowTurn]) -> Vec<String> {
    quoted_references(&[shown("lesson:a", passage)], turns)
}

#[test]
fn words_splits_on_anything_that_is_not_a_letter_or_digit_and_lowercases() {
    assert_eq!(
        words("Don't STOP: 42-times, ok?"),
        vec!["don", "t", "stop", "42", "times", "ok"]
    );
}

#[test]
fn quoted_references_answer_repeating_eight_words_marks_the_reference() {
    let answer = "As found, reranker passages exceed budget because candidates arrive unsorted during the run.";

    assert_eq!(quoted(PASSAGE, &[turn(answer, "")]), vec!["lesson:a"]);
}

#[test]
fn quoted_references_ignores_case_and_punctuation_when_comparing() {
    let answer =
        "RERANKER, passages -- exceed budget!! Because candidates (arrive) unsorted; during warmup";

    assert_eq!(quoted(PASSAGE, &[turn(answer, "")]), vec!["lesson:a"]);
}

#[test]
fn quoted_references_answer_repeating_only_seven_words_marks_nothing() {
    let answer = "reranker passages exceed budget because candidates arrive and then something else entirely";

    assert!(quoted(PASSAGE, &[turn(answer, "")]).is_empty());
}

#[test]
fn quoted_references_eight_short_words_under_forty_characters_mark_nothing() {
    let passage = "one two six ten and the cat sat down by the old gate";
    let answer = "we saw one two six ten and the cat sat there";

    assert!(quoted(passage, &[turn(answer, "")]).is_empty());
}

#[test]
fn quoted_references_run_that_is_also_in_the_turns_prompt_marks_nothing() {
    let answer = "reranker passages exceed budget because candidates arrive unsorted during warmup";
    let prompt = "Why do the reranker passages exceed budget because candidates arrive unsorted during warmup?";

    assert!(quoted(PASSAGE, &[turn(answer, prompt)]).is_empty());
}

#[test]
fn quoted_references_prompt_of_another_turn_does_not_hide_the_quote() {
    let answer = "reranker passages exceed budget because candidates arrive unsorted during warmup";
    let prompt = "reranker passages exceed budget because candidates arrive unsorted during warmup";
    let turns = [
        turn("nothing relevant here", prompt),
        turn(answer, "continue"),
    ];

    assert_eq!(quoted(PASSAGE, &turns), vec!["lesson:a"]);
}

#[test]
fn quoted_references_only_reads_the_first_1500_characters_of_the_shown_text() {
    let padding = "filler ".repeat(SHOWN_CHARS / 7 + 1);
    let passage = format!("{padding}{PASSAGE}");
    let answer = "reranker passages exceed budget because candidates arrive unsorted during warmup";

    assert!(quoted(&passage, &[turn(answer, "")]).is_empty());
}

#[test]
fn quoted_references_only_reads_the_first_ten_shown_texts() {
    let answer = "reranker passages exceed budget because candidates arrive unsorted during warmup";
    let texts: Vec<ShownText> = (0..=SHOWN_PER_EVENT)
        .map(|n| shown(&format!("lesson:{n}"), PASSAGE))
        .collect();

    let found = quoted_references(&texts, &[turn(answer, "")]);

    assert_eq!(found.len(), SHOWN_PER_EVENT);
    assert!(!found.contains(&format!("lesson:{SHOWN_PER_EVENT}")));
}

#[test]
fn quoted_references_without_shown_text_or_turns_marks_nothing() {
    assert!(quoted_references(&[], &[turn("anything", "")]).is_empty());
    assert!(quoted(PASSAGE, &[]).is_empty());
}
