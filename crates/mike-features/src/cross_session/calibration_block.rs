use chrono::{DateTime, Utc};
use serde::Serialize;

use super::snapshot::CrossSessionSnapshot;
use crate::store::persistence::cross_session::CalibrationRateRow;

pub const CALIBRATION_BLOCK_VERSION: &str = "cal-v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationLine {
    pub question: String,
    pub scope: &'static str,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationBlock {
    pub version: &'static str,
    pub rollup_id: Option<i64>,
    pub lines: Vec<CalibrationLine>,
    pub text: String,
}

fn count(row: Option<&CalibrationRateRow>) -> Option<(i32, i32)> {
    row.map(|r| (r.k, r.n))
}

fn ci(row: &CalibrationRateRow) -> String {
    match (row.ci_low, row.ci_high) {
        (Some(l), Some(h)) => format!(" (95% CI {l:.2}-{h:.2})"),
        _ => String::new(),
    }
}

fn fact_for(question: &str) -> Option<&'static str> {
    match question {
        "relapse" => Some("after_corrected_turn"),
        "unobserved_done" => Some("unobserved_after_change"),
        _ => None,
    }
}

fn base_rates(
    s: &CrossSessionSnapshot,
    question: &str,
    project_id: Option<i64>,
    where_: &str,
) -> String {
    let mut parts = Vec::new();
    if let Some((k, n)) = count(s.base_rate("reaction", Some("ended_on"), "all", project_id)) {
        parts.push(format!(
            "the operator corrected {k} of {n} replied turns {where_}"
        ));
    }
    if let Some(fact) = fact_for(question)
        && let Some((k, n)) = count(s.base_rate("reaction", Some("guard_fact"), fact, project_id))
    {
        let what = if fact == "after_corrected_turn" {
            "after a turn the operator corrected, the next turn was corrected"
        } else {
            "after a change nothing observed, the turn was corrected"
        };
        parts.push(format!("{what} {k} of {n} times"));
    }
    if let Some((k, n)) = count(s.base_rate("session_end", None, "all", project_id)) {
        parts.push(format!(
            "{k} of {n} session ends {where_} closed in frustration"
        ));
    }
    parts.join("; ")
}

fn line(
    s: &CrossSessionSnapshot,
    question: &str,
    project_id: Option<i64>,
    threshold: f64,
) -> CalibrationLine {
    let min = s.settings.calibration_min_labelled;
    let project = project_id
        .and_then(|p| s.candidate_row(question, Some(p)))
        .filter(|r| i64::from(r.n) >= min);
    let global = s
        .candidate_row(question, None)
        .filter(|r| i64::from(r.n) >= min);
    let (scope, row, where_, scope_project) = match (project, global) {
        (Some(r), _) => ("project", Some(r), "in this project", project_id),
        (None, Some(r)) => ("global", Some(r), "in all projects", None),
        (None, None) => ("none", None, "in all projects", None),
    };
    let head = match row {
        Some(r) => format!(
            "{question}, {}: candidates at confidence {threshold:.2} or above were confirmed {} of {}{}",
            if scope == "project" {
                "this project"
            } else {
                "all projects"
            },
            r.k,
            r.n,
            ci(r)
        ),
        None => {
            let seen = s.candidate_row(question, None).map_or(0, |r| r.n);
            format!("{question}, all projects: too few labelled candidates ({seen}, need {min})")
        }
    };
    let rates = base_rates(s, question, scope_project, where_);
    let text = if rates.is_empty() {
        format!("- {head}.")
    } else {
        format!("- {head}; {rates}.")
    };
    CalibrationLine {
        question: question.to_owned(),
        scope,
        text,
    }
}

pub fn calibration_block(
    s: &CrossSessionSnapshot,
    open_questions: &[(&str, f64)],
    project_id: Option<i64>,
    as_of: DateTime<Utc>,
) -> Option<CalibrationBlock> {
    if s.settings.input_mode != "live" || open_questions.is_empty() {
        return None;
    }
    let lines: Vec<CalibrationLine> = open_questions
        .iter()
        .map(|(q, t)| line(s, q, project_id, *t))
        .collect();
    let header = format!(
        "[calibration {CALIBRATION_BLOCK_VERSION} as_of={} window={}d]",
        as_of.format("%Y-%m-%dT%H:%M:%SZ"),
        s.settings.calibration_window_days
    );
    let body: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    Some(CalibrationBlock {
        version: CALIBRATION_BLOCK_VERSION,
        rollup_id: s.rollup.map(|r| r.id),
        text: format!("{header}\n{}", body.join("\n")),
        lines,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn rate(
        family: &str,
        question: Option<&str>,
        project: Option<i64>,
        dimension: (&str, &str),
        (k, n): (i32, i32),
    ) -> CalibrationRateRow {
        CalibrationRateRow {
            family: family.to_owned(),
            metric: "precision".to_owned(),
            population: "steerer_evaluations".to_owned(),
            scope_kind: if project.is_some() {
                "project"
            } else {
                "global"
            }
            .to_owned(),
            project_id: project,
            axis: if family == "base_rate" {
                "reaction"
            } else {
                "steer_candidate"
            }
            .to_owned(),
            class: "applies".to_owned(),
            dimension: Some(dimension.0.to_owned()),
            dimension_value: Some(dimension.1.to_owned()),
            question: question.map(str::to_owned),
            mode: Some("shadow".to_owned()),
            k,
            n,
            value: Some(f64::from(k) / f64::from(n)),
            ci_low: Some(0.6),
            ci_high: Some(0.98),
            detail: serde_json::json!({}),
        }
    }

    fn snapshot() -> CrossSessionSnapshot {
        let mut s = CrossSessionSnapshot::default();
        s.settings.input_mode = "live".to_owned();
        s.rows = vec![
            rate(
                "steer_calibration",
                Some("unbacked_wait"),
                Some(7),
                ("all", "all"),
                (9, 20),
            ),
            rate(
                "steer_calibration",
                Some("relapse"),
                None,
                ("all", "all"),
                (4, 4),
            ),
            rate("base_rate", None, None, ("ended_on", "all"), (412, 1003)),
        ];
        s
    }

    #[test]
    fn calibration_block_off_mode_adds_nothing() {
        let mut s = snapshot();
        s.settings.input_mode = "off".to_owned();
        assert!(calibration_block(&s, &[("relapse", 0.75)], Some(7), Utc::now()).is_none());
    }

    #[test]
    fn a_project_with_fewer_than_min_labelled_gets_the_global_line() {
        let s = snapshot();
        let b = calibration_block(
            &s,
            &[("unbacked_wait", 0.75), ("relapse", 0.75)],
            Some(7),
            Utc::now(),
        );
        let Some(b) = b else {
            panic!("no block in live mode");
        };
        assert_eq!(b.lines[0].scope, "project");
        assert!(b.lines[0].text.contains("confirmed 9 of 20"));
        assert_eq!(b.lines[1].scope, "none");
        assert!(
            b.lines[1]
                .text
                .contains("too few labelled candidates (4, need 20)")
        );
        assert!(b.text.starts_with("[calibration cal-v1 as_of="));
    }
}
