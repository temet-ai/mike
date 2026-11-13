use super::Question;
use super::label_rules::{LabelInput, Settlement, from_answer, is_prod, settle};

fn input(question: Question, would_fire: bool) -> LabelInput<'static> {
    LabelInput {
        question,
        would_fire,
        actor_primary: true,
        reaction: None,
        session_end: None,
        exempt: None,
        superseded_by: None,
        has_next_message: true,
    }
}

fn label_of(s: &Settlement) -> Option<&'static str> {
    match s {
        Settlement::Label { label, .. } => Some(label),
        _ => None,
    }
}

#[test]
fn continue_prod_after_wait_fire_goes_to_label_call() {
    let i = LabelInput {
        reaction: Some("accepted"),
        ..input(Question::UnbackedWait, true)
    };
    assert_eq!(settle(&i), Settlement::NeedsCall);
    assert!(is_prod("continue"));
}

#[test]
fn accepted_reaction_on_relapse_goes_to_label_call() {
    let i = LabelInput {
        reaction: Some("redirected"),
        ..input(Question::Relapse, true)
    };
    assert_eq!(settle(&i), Settlement::NeedsCall);
}

#[test]
fn accepted_reaction_on_done_is_settled_without_call() {
    let fire = LabelInput {
        reaction: Some("accepted"),
        ..input(Question::UnobservedDone, true)
    };
    let pass = LabelInput {
        reaction: Some("accepted"),
        ..input(Question::UnobservedDone, false)
    };
    assert_eq!(label_of(&settle(&fire)), Some("wrong"));
    assert_eq!(label_of(&settle(&pass)), Some("right"));
}

#[test]
fn unclear_reaction_is_indeterminate() {
    let i = LabelInput {
        reaction: Some("unclear"),
        ..input(Question::AuthorisedHandback, true)
    };
    assert_eq!(label_of(&settle(&i)), Some("indeterminate"));
}

#[test]
fn non_primary_actor_would_fire_is_wrong() {
    let i = LabelInput {
        actor_primary: false,
        ..input(Question::Relapse, true)
    };
    assert_eq!(label_of(&settle(&i)), Some("wrong"));
}

#[test]
fn prod_list_v1_matches_short_prods() {
    assert!(is_prod("?"));
    assert!(is_prod("status?"));
    assert!(is_prod("any update?"));
    assert!(!is_prod(
        "no, the retry is wrong because it swallows the timeout error entirely"
    ));
}

#[test]
fn superseded_rows_are_indeterminate() {
    let i = LabelInput {
        superseded_by: Some(uuid::Uuid::nil()),
        ..input(Question::Relapse, true)
    };
    assert_eq!(label_of(&settle(&i)), Some("indeterminate"));
}

#[test]
fn superseded_is_per_trigger_point() {
    let turn_end = input(Question::UnbackedWait, true);
    let quiet = LabelInput {
        superseded_by: None,
        ..input(Question::UnbackedWait, false)
    };
    assert_eq!(settle(&turn_end), Settlement::NeedsCall);
    assert_eq!(settle(&quiet), Settlement::NeedsCall);
}

#[test]
fn exempt_rows_are_indeterminate_without_call() {
    let i = LabelInput {
        exempt: Some("session_class:fleet_seat"),
        ..input(Question::UnbackedWait, false)
    };
    assert_eq!(label_of(&settle(&i)), Some("indeterminate"));
}

#[test]
fn frustration_close_marks_relapse_fire_right() {
    let i = LabelInput {
        reaction: Some("no_response"),
        session_end: Some("closed_in_frustration"),
        ..input(Question::Relapse, true)
    };
    assert_eq!(label_of(&settle(&i)), Some("right"));
    let done = LabelInput {
        reaction: Some("no_response"),
        session_end: Some("done"),
        ..input(Question::UnbackedWait, true)
    };
    assert_eq!(label_of(&settle(&done)), Some("wrong"));
}

#[test]
fn label_call_answers_map_to_labels() {
    assert_eq!(from_answer(true, "yes"), "right");
    assert_eq!(from_answer(false, "yes"), "wrong");
    assert_eq!(from_answer(true, "unclear"), "indeterminate");
}
