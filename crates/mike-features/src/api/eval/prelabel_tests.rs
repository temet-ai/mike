use super::*;
use crate::llm::decisions::Answer;

fn turn(id: i64) -> PrelabelTurn {
    PrelabelTurn {
        turn_id: id,
        session_id: 3,
        user_prompt: "fix the bug".to_owned(),
        final_text: "fixed it".to_owned(),
        next_prompt: "no, that is wrong".to_owned(),
    }
}

fn pick(key: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: BTreeMap::from([(key.to_owned(), p)]),
        confidence: None,
    }
}

#[test]
fn compose_marks_a_confident_label_sure_and_a_weak_one_uncertain() {
    let turns = [turn(1), turn(2)];
    let group: Vec<&PrelabelTurn> = turns.iter().collect();
    let asked = Asked::answered(BTreeMap::from([
        (question_key(1), pick("corrected", 0.95)),
        (question_key(2), pick("accepted", 0.55)),
    ]));
    let found = compose(&asked, &group, Some(9));
    assert_eq!(found.len(), 2);
    assert!(!found[0].1.uncertain);
    assert_eq!(found[0].1.label, "corrected");
    assert!(found[1].1.uncertain);
    assert_eq!(found[1].1.record_id, Some(9));
}

#[test]
fn compose_unknown_label_or_failed_answer_gives_no_prelabel() {
    let turns = [turn(1)];
    let group: Vec<&PrelabelTurn> = turns.iter().collect();
    let invented = Asked::answered(BTreeMap::from([(question_key(1), pick("angry", 0.9))]));
    assert!(compose(&invented, &group, None).is_empty());
    assert!(compose(&Asked::failed_with("timeout"), &group, None).is_empty());
}

#[test]
fn attach_adds_the_prelabel_only_to_items_that_have_one() {
    let prelabels = BTreeMap::from([(
        1,
        Prelabel {
            label: "corrected".to_owned(),
            probability: 0.9,
            uncertain: false,
            record_id: Some(4),
        },
    )]);
    let items = vec![
        json!({ "operator_turn_id": 1, "outcome": "x" }),
        json!({ "operator_turn_id": 2, "outcome": "y" }),
    ];
    let out = attach(items, &prelabels);
    assert_eq!(out[0]["prelabel"]["label"], "corrected");
    assert_eq!(out[0]["prelabel"]["uncertain"], false);
    assert!(out[0]["prelabel"].get("record_id").is_none());
    assert!(out[1].get("prelabel").is_none());
}

#[test]
fn state_and_questions_cover_each_turn_with_the_reaction_vocabulary() -> anyhow::Result<()> {
    let turns = [turn(5)];
    let group: Vec<&PrelabelTurn> = turns.iter().collect();
    assert_eq!(
        state(&group)["turns"][0]["operator_next_message"],
        "no, that is wrong"
    );
    let asked = questions(&group);
    let Some(Question::Choice { criteria, .. }) = asked.get("turn_5") else {
        anyhow::bail!("expected a choice question");
    };
    assert_eq!(criteria.len(), REACTIONS.len() - 1);
    assert!(!criteria.contains_key(NO_RESPONSE));
    Ok(())
}

fn silent_turn(id: i64, next: &str) -> PrelabelTurn {
    PrelabelTurn {
        next_prompt: next.to_owned(),
        ..turn(id)
    }
}

#[test]
fn a_turn_without_a_next_message_is_set_no_response_in_code_and_not_asked() {
    let turns = [
        silent_turn(7_001, ""),
        silent_turn(7_002, "  \n\t "),
        silent_turn(7_003, "thanks, now the docs"),
    ];
    let speaking = settle_silent(&turns);
    assert_eq!(speaking.len(), 1);
    assert_eq!(speaking[0].turn_id, 7_003);
    let stored = cached(&[7_001, 7_002, 7_003]);
    assert_eq!(stored.len(), 2);
    for id in [7_001, 7_002] {
        assert_eq!(stored[&id].label, "no_response");
        assert!(!stored[&id].uncertain);
        assert_eq!(stored[&id].record_id, None);
    }
}

#[test]
fn state_reads_a_whitespace_next_message_as_none() {
    let turns = [silent_turn(7_010, "  ")];
    let group: Vec<&PrelabelTurn> = turns.iter().collect();
    assert_eq!(
        state(&group)["turns"][0]["operator_next_message"],
        Value::Null
    );
}
