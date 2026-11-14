use super::*;
use crate::llm::decisions::Answer;

fn row(id: i64, text: &str, topic: Option<&str>) -> UncheckedLesson {
    UncheckedLesson {
        id,
        text: text.to_owned(),
        topic: topic.map(str::to_owned),
    }
}

fn status(chars: usize) -> Assessment {
    Assessment {
        shape: Shape::Status,
        detail: "status".to_owned(),
        chars,
    }
}

fn answered(id: i64, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        question_key(id),
        Answer::Noul {
            noul: p,
            confidence: None,
        },
    )]))
}

#[test]
fn candidates_keep_only_status_openers_outside_verifier_topics() {
    let rows = vec![
        row(1, "Done means the tests pass.", None),
        row(2, "Done with the migration.", Some("verifier:x")),
        row(3, "Always run the check.", None),
    ];
    let findings = vec![(1, status(26)), (2, status(24)), (3, status(21))];
    let found = candidates(&rows, &findings);
    assert_eq!(found.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1]);
}

#[test]
fn compose_rule_answer_above_the_floor_clears_the_lesson() {
    let rows = [row(1, "Done means the tests pass.", None)];
    let group: Vec<&UncheckedLesson> = rows.iter().collect();
    assert_eq!(compose(&answered(1, 0.9), &group), vec![(1, 0.9)]);
    assert!(compose(&answered(1, 0.4), &group).is_empty());
}

#[test]
fn compose_failed_answer_keeps_the_status_finding() {
    let rows = [row(1, "Done means the tests pass.", None)];
    let group: Vec<&UncheckedLesson> = rows.iter().collect();
    assert!(compose(&Asked::failed_with("timeout"), &group).is_empty());
}

#[test]
fn applied_score_is_the_probability_of_the_action_taken() {
    let rows = [row(1, "Done means x.", None)];
    let group: Vec<&UncheckedLesson> = rows.iter().collect();
    assert!((applied_score(&answered(1, 0.9), &group) - 0.9).abs() < 1e-9);
    assert!((applied_score(&answered(1, 0.2), &group) - 0.8).abs() < 1e-9);
}

#[test]
fn state_and_questions_cover_every_lesson_in_the_group() {
    let rows = [row(7, "Done means x.", Some("t"))];
    let group: Vec<&UncheckedLesson> = rows.iter().collect();
    assert_eq!(state(&group)["lessons"][0]["id"], 7);
    assert!(questions(&group).contains_key("lesson_7"));
}
