use super::{CriteriaFields, CriteriaInputs};
use crate::store::persistence::acceptance_criteria::{
    AcceptanceCriteriaMap, AcceptanceCriterionRow,
};
use mike_launch::store::persistence::work_threads::criteria::ThreadCriteria;

fn own_map() -> AcceptanceCriteriaMap {
    AcceptanceCriteriaMap {
        id: 1,
        version: 1,
        ask_entry_count: 1,
        model: None,
        output_tokens: None,
        created_at: chrono::Utc::now(),
        criteria: vec![AcceptanceCriterionRow {
            ord: 0,
            criterion: "This session's own criterion.".to_string(),
            provenance: "operator".to_string(),
            operator_span: Some("my words".to_string()),
        }],
    }
}

fn linked(slug: &str) -> ThreadCriteria {
    ThreadCriteria {
        thread_slug: slug.to_string(),
        title: format!("{slug} title"),
        criteria_text: format!("Criteria:\n- {slug} criterion."),
    }
}

fn texts(fields: &CriteriaFields) -> Vec<&str> {
    fields
        .checklist_criteria
        .iter()
        .map(|c| c.text.as_str())
        .collect()
}

#[test]
fn by_default_uses_the_sessions_own_rows_and_never_a_linked_thread() {
    let inputs = CriteriaInputs::new(vec![linked("a"), linked("b")], Some(own_map()));
    let fields = inputs.by_default();
    assert!(fields.thread_criteria.is_none());
    assert!(
        fields
            .intake_criteria
            .as_deref()
            .is_some_and(|t| t.contains("[operator asked] This session's own criterion."))
    );
    assert_eq!(texts(&fields), vec!["This session's own criterion."]);
    assert!(!fields.checklist_criteria[0].inferred);
}

#[test]
fn accepted_threads_feed_the_judge_and_the_checklist_the_same_criteria() {
    let inputs = CriteriaInputs::new(vec![linked("a"), linked("b"), linked("c")], Some(own_map()));
    let fields = inputs.fields(&[0, 2]);
    assert_eq!(
        fields.thread_criteria.as_deref(),
        Some(
            "Work thread a title:\nCriteria:\n- a criterion.\n\nWork thread c title:\nCriteria:\n- c criterion."
        )
    );
    assert!(fields.intake_criteria.is_none());
    assert_eq!(texts(&fields), vec!["a criterion.", "c criterion."]);
}

#[test]
fn no_own_rows_and_no_accepted_thread_gives_no_criteria() {
    let empty_map = AcceptanceCriteriaMap {
        criteria: Vec::new(),
        ..own_map()
    };
    let inputs = CriteriaInputs::new(vec![linked("a")], Some(empty_map));
    assert_eq!(inputs.by_default(), CriteriaFields::default());
    assert_eq!(inputs.fields(&[7]), CriteriaFields::default());
}
