use serde_json::Value;

use super::run::{Outcome, Sample};
use super::summary::summarize;

fn sample(
    score: f64,
    positive: Option<bool>,
    right: Option<bool>,
    current: Option<bool>,
) -> Sample {
    Sample {
        score,
        positive,
        label_right: right,
        agrees_current: current,
    }
}

fn outcome(questions: Vec<(&str, Sample)>, composed: Option<Sample>, failed: bool) -> Outcome {
    Outcome {
        line: Value::Null,
        failed,
        questions: questions
            .into_iter()
            .map(|(q, s)| (q.to_owned(), s))
            .collect(),
        composed,
    }
}

#[test]
fn summary_computes_auroc_and_agreement_per_question_and_composed() {
    let outcomes = vec![
        outcome(
            vec![("q", sample(0.9, Some(true), Some(true), Some(true)))],
            Some(sample(0.8, Some(true), Some(true), Some(true))),
            false,
        ),
        outcome(
            vec![("q", sample(0.2, Some(false), Some(true), Some(false)))],
            Some(sample(0.3, Some(false), Some(true), Some(false))),
            false,
        ),
        outcome(
            vec![("q", sample(0.6, Some(false), Some(false), None))],
            Some(sample(0.6, Some(false), Some(false), None)),
            false,
        ),
        outcome(Vec::new(), None, true),
    ];
    let summary = summarize("p", &outcomes, None);
    assert_eq!(summary.items, 4);
    assert_eq!(summary.failed, 1);
    let q = &summary.questions[0];
    assert_eq!(q.name, "q");
    assert_eq!(q.count, 3);
    assert_eq!(q.auroc, Some(1.0));
    assert_eq!(q.label_agreement.map(|v| (v * 3.0).round()), Some(2.0));
    assert_eq!(q.current_agreement, Some(0.5));
    assert_eq!(q.current_count, 2);
    assert_eq!(summary.composed.count, 3);
    assert_eq!(summary.composed.auroc, Some(1.0));
}

#[test]
fn summary_without_both_classes_has_no_auroc() {
    let outcomes = vec![outcome(
        vec![("q", sample(0.9, Some(true), Some(true), None))],
        Some(sample(0.9, Some(true), Some(true), None)),
        false,
    )];
    let summary = summarize("p", &outcomes, Some("ceiling".to_owned()));
    assert_eq!(summary.questions[0].auroc, None);
    assert_eq!(summary.composed.current_agreement, None);
    assert_eq!(summary.composed.current_count, 0);
    assert_eq!(summary.stopped.as_deref(), Some("ceiling"));
}

#[test]
fn summary_of_no_items_is_empty() {
    let summary = summarize("p", &[], None);
    assert_eq!(summary.items, 0);
    assert!(summary.questions.is_empty());
    assert_eq!(summary.composed.count, 0);
    assert_eq!(summary.composed.label_agreement, None);
}
