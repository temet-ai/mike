use serde_json::json;

use super::*;

fn candidate(id: i64, label: &'static str) -> Candidate {
    Candidate {
        key: format!("test_queue:{id}"),
        question_key: format!("proposal_{id}"),
        label,
        item: json!({ "id": id, "text": format!("item {id}") }),
    }
}

fn pick(key: &str, probabilities: &[(&str, f64)]) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: probabilities
            .iter()
            .map(|(k, p)| ((*k).to_owned(), *p))
            .collect(),
        confidence: None,
    }
}

#[test]
fn compose_reads_the_accept_probability_and_the_top_choice() {
    let items = [candidate(1, "proposal"), candidate(2, "proposal")];
    let group: Vec<&Candidate> = items.iter().collect();
    let asked = Asked::answered(BTreeMap::from([
        (
            "proposal_1".to_owned(),
            pick("accept", &[("accept", 0.8), ("reject", 0.2)]),
        ),
        (
            "proposal_2".to_owned(),
            pick("duplicate", &[("accept", 0.1), ("duplicate", 0.7)]),
        ),
    ]));
    assert_eq!(
        compose(&asked, &group),
        vec![
            ("test_queue:1".to_owned(), 0.8, "accept".to_owned()),
            ("test_queue:2".to_owned(), 0.1, "duplicate".to_owned())
        ]
    );
}

#[test]
fn compose_failed_answer_gives_no_guesses() {
    let items = [candidate(1, "proposal")];
    let group: Vec<&Candidate> = items.iter().collect();
    assert!(compose(&Asked::failed_with("timeout"), &group).is_empty());
}

#[test]
fn questions_name_the_kind_of_item_being_asked_about() {
    let items = [candidate(7, "skill draft")];
    let group: Vec<&Candidate> = items.iter().collect();
    let asked = questions(&group);
    let Some(Question::Choice { instructions, .. }) = asked.get("proposal_7") else {
        panic!("expected a choice question for item 7");
    };
    assert!(instructions.contains("skill draft 7"));
    assert_eq!(state(&group)["proposals"][0]["id"], 7);
}

#[test]
fn label_for_compares_the_top_choice_with_the_operator_decision() {
    assert_eq!(label_for("accept", true), "right");
    assert_eq!(label_for("accept", false), "wrong");
    assert_eq!(label_for("duplicate", false), "right");
    assert_eq!(label_for("reject", true), "wrong");
}

#[test]
fn spawn_fill_without_a_decision_env_leaves_no_guess() {
    spawn_fill(None, vec![candidate(900_010, "proposal")]);
    assert!(cached(["test_queue:900010"]).is_empty());
    assert!(!FILLING.contains("test_queue:900010"));
}

#[test]
fn cached_returns_only_keys_that_have_a_guess() {
    GUESSES.insert(
        "test_queue:cached".to_owned(),
        Guess {
            accept: 0.6,
            top: "accept".to_owned(),
            record_id: None,
        },
    );
    let found = cached(["test_queue:cached", "test_queue:absent"]);
    assert_eq!(
        found,
        BTreeMap::from([("test_queue:cached".to_owned(), 0.6)])
    );
}
