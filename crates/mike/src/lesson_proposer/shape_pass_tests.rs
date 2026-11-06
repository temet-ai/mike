use super::{TwinDecision, kept_after_comparison};
use crate::lesson_proposer::relate::Relation;
use crate::store::persistence::lessons::shape_apply::RewrittenRule;

fn rule(text: &str) -> RewrittenRule {
    RewrittenRule {
        text: text.into(),
        embedding: None,
    }
}

fn decision(lesson_id: i64, relation: Relation, reason: &str) -> TwinDecision {
    TwinDecision {
        lesson_id,
        relation,
        reason: reason.into(),
    }
}

#[test]
fn a_rule_the_model_says_repeats_an_active_lesson_is_dropped_with_the_reason_noted() {
    let rules = vec![
        rule("sweep every target folder"),
        rule("lock the profile first"),
    ];
    let decisions = vec![
        vec![decision(
            2004,
            Relation::Skip,
            "lesson 2004 already says this",
        )],
        vec![decision(2010, Relation::Store, "a different step")],
    ];
    let (kept, notes) = kept_after_comparison(rules, &decisions);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].text, "lock the profile first");
    assert_eq!(
        notes,
        vec![
            "rule 1 not stored, it repeats lesson 2004: lesson 2004 already says this".to_string()
        ]
    );
}

#[test]
fn rules_without_a_skip_decision_are_all_kept_in_order() {
    let rules = vec![rule("a"), rule("b"), rule("c")];
    let decisions = vec![
        Vec::new(),
        vec![decision(7, Relation::Update, "newer wording")],
    ];
    let (kept, notes) = kept_after_comparison(rules, &decisions);
    assert_eq!(
        kept.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert!(notes.is_empty());
}
