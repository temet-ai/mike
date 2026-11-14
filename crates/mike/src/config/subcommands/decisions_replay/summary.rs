use std::collections::BTreeMap;

use serde::Serialize;

use super::run::{Outcome, Sample};
use crate::steerer::precheck::score::auroc;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct Row {
    pub name: String,
    pub count: usize,
    pub auroc: Option<f64>,
    pub label_agreement: Option<f64>,
    pub current_agreement: Option<f64>,
    pub current_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct Summary {
    pub point: String,
    pub items: usize,
    pub failed: usize,
    pub stopped: Option<String>,
    pub questions: Vec<Row>,
    pub composed: Row,
}

fn share(flags: impl Iterator<Item = bool>) -> (Option<f64>, usize) {
    let (yes, all) = flags.fold((0usize, 0usize), |(yes, all), flag| {
        (yes + usize::from(flag), all + 1)
    });
    (((all > 0).then(|| yes as f64 / all as f64)), all)
}

fn row(name: &str, samples: &[&Sample]) -> Row {
    let scored: Vec<(f64, bool)> = samples
        .iter()
        .filter_map(|s| s.positive.map(|p| (s.score, p)))
        .collect();
    let (label_agreement, _) = share(samples.iter().filter_map(|s| s.label_right));
    let (current_agreement, current_count) = share(samples.iter().filter_map(|s| s.agrees_current));
    Row {
        name: name.to_owned(),
        count: samples.len(),
        auroc: auroc(&scored),
        label_agreement,
        current_agreement,
        current_count,
    }
}

pub(super) fn summarize(point: &str, outcomes: &[Outcome], stopped: Option<String>) -> Summary {
    let mut by_question: BTreeMap<&str, Vec<&Sample>> = BTreeMap::new();
    for outcome in outcomes {
        for (question, sample) in &outcome.questions {
            by_question.entry(question).or_default().push(sample);
        }
    }
    let composed: Vec<&Sample> = outcomes
        .iter()
        .filter_map(|o| o.composed.as_ref())
        .collect();
    Summary {
        point: point.to_owned(),
        items: outcomes.len(),
        failed: outcomes.iter().filter(|o| o.failed).count(),
        stopped,
        questions: by_question
            .iter()
            .map(|(question, samples)| row(question, samples))
            .collect(),
        composed: row("composed", &composed),
    }
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |v| format!("{v:.3}"))
}

fn line(row: &Row) -> String {
    format!(
        "  {:28} count {:5}  AUROC {:>6}  agrees with label {:>6}  agrees with current judge {:>6} (of {})",
        row.name,
        row.count,
        percent(row.auroc),
        percent(row.label_agreement),
        percent(row.current_agreement),
        row.current_count
    )
}

pub(super) fn print(summary: &Summary) {
    println!(
        "{}: {} items asked, {} failed",
        summary.point, summary.items, summary.failed
    );
    if let Some(reason) = &summary.stopped {
        println!("  stopped early: {reason}");
    }
    for row in &summary.questions {
        println!("{}", line(row));
    }
    println!("{}", line(&summary.composed));
}
