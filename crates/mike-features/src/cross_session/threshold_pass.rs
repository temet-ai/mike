use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};

use super::fire_pass::plan_fire_rows;
use super::snapshot::CrossSessionSnapshot;
use super::threshold::{DecisionInputs, calibrate_threshold, decide_threshold};
use super::weighting::{BinInput, Weighted, bin_input, by_source_json, weigh};
use crate::features_settings::cross_session::CrossSessionSettings;
use crate::steerer::guard::Question;
use crate::steerer::guard::bars::promotion_bar;
use crate::steerer::guard::policy::{GuardConfig, QuestionMode, question_modes};
use crate::store::persistence::cross_session::{
    RollupRef, ThresholdPassRunWrite, ThresholdRow, ThresholdWrite,
};
use crate::store::persistence::{PersistError, PersistHandle};

pub fn last_applied(
    history: &[ThresholdRow],
    question: &str,
    project_id: Option<i64>,
) -> Option<(f32, DateTime<Utc>)> {
    history
        .iter()
        .filter(|t| t.question == question && t.project_id == project_id && t.decision == "applied")
        .max_by_key(|t| (t.computed_at, t.id))
        .map(|t| (t.applied_threshold, t.computed_at))
}

pub struct RowSpec<'a> {
    pub steerer_id: i64,
    pub question: &'a str,
    pub project: Option<i64>,
    pub rollup: RollupRef,
    pub question_mode: &'a str,
    pub bar: f64,
    pub floor: f64,
    pub static_threshold: f32,
    pub previous: Option<(f32, DateTime<Utc>)>,
    pub steerer_name: Option<&'a str>,
    pub now: DateTime<Utc>,
}

pub fn build_row(
    settings: &CrossSessionSettings,
    spec: &RowSpec<'_>,
    weighted: &Weighted,
) -> ThresholdWrite {
    let min_n = settings.threshold_min_labelled as f64;
    let candidate = calibrate_threshold(
        &weighted.bins,
        spec.bar,
        spec.floor,
        settings.ceiling,
        min_n,
    );
    let current = spec.previous.map_or(spec.static_threshold, |p| p.0);
    let decision = decide_threshold(&DecisionInputs {
        current,
        static_threshold: spec.static_threshold,
        candidate,
        enough_labelled: weighted.n_weighted >= min_n,
        mode: &settings.threshold_mode,
        hysteresis: settings.hysteresis,
        cooldown_days: settings.cooldown_days,
        last_applied_at: spec.previous.map(|p| p.1),
        question_mode: spec.question_mode,
        allow_lowering_for_wake: settings.allow_lowering_for_wake,
        now: spec.now,
    });
    let applied = if decision.code() == "applied" {
        candidate.unwrap_or(current)
    } else {
        current
    };
    let mut detail = json!({
        "bins": weighted.bins,
        "previous_threshold": current,
        "by_source": by_source_json(&weighted.totals, settings),
        "n_weighted": weighted.n_weighted,
    });
    if let (Some(name), Some(map)) = (spec.steerer_name, detail.as_object_mut()) {
        map.insert("steerer".to_owned(), Value::String(name.to_owned()));
    }
    ThresholdWrite {
        steerer_id: spec.steerer_id,
        question: spec.question.to_owned(),
        scope: if spec.project.is_some() {
            "project"
        } else {
            "global"
        }
        .to_owned(),
        project_id: spec.project,
        rollup_id: spec.rollup.id,
        rollup_finished_at: spec.rollup.finished_at,
        window_start: spec.now - Duration::days(30),
        window_end: spec.now,
        label_basis: "weighted_by_source".to_owned(),
        question_mode: spec.question_mode.to_owned(),
        threshold_mode: settings.threshold_mode.clone(),
        n_labelled: i32::try_from(weighted.n_raw).unwrap_or(i32::MAX),
        precision_bar: spec.bar,
        threshold_floor: spec.floor as f32,
        threshold_ceiling: settings.ceiling as f32,
        static_threshold: spec.static_threshold,
        candidate_threshold: candidate,
        applied_threshold: applied,
        decision: decision.code().to_owned(),
        detail,
    }
}

pub struct PassInputs<'a> {
    pub snapshot: &'a CrossSessionSnapshot,
    pub config: &'a GuardConfig,
    pub question_modes: &'a HashMap<Question, QuestionMode>,
    pub now: DateTime<Utc>,
}

pub fn plan_threshold_rows(i: &PassInputs<'_>) -> Vec<ThresholdWrite> {
    let s = i.snapshot;
    let (Some(rollup), Some(steerer_id)) = (s.rollup, s.guard_steerer_id) else {
        return Vec::new();
    };
    let mut scopes: Vec<Option<i64>> = s
        .rows
        .iter()
        .filter(|r| r.family == "steer_calibration")
        .map(|r| r.project_id)
        .collect();
    scopes.sort_unstable();
    scopes.dedup();
    let mut out = Vec::new();
    for question in Question::ALL {
        let params = i.config.questions.get(&question).copied();
        let mode = i
            .question_modes
            .get(&question)
            .map_or("shadow", |m| m.code());
        let static_threshold = params.map_or(0.7, |p| p.confidence_threshold) as f32;
        for project in &scopes {
            let inputs: Vec<BinInput> = s
                .bins(question.code(), *project)
                .into_iter()
                .filter_map(bin_input)
                .collect();
            let weighted = weigh(&inputs, &s.settings);
            out.push(build_row(
                &s.settings,
                &RowSpec {
                    steerer_id,
                    question: question.code(),
                    project: *project,
                    rollup,
                    question_mode: mode,
                    bar: promotion_bar(question),
                    floor: params.map_or(0.6, |p| p.floor),
                    static_threshold,
                    previous: last_applied(&s.thresholds, question.code(), *project),
                    steerer_name: None,
                    now: i.now,
                },
                &weighted,
            ));
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct PassSummary {
    pub changes: Vec<i64>,
    pub outcome: &'static str,
    pub rows_written: usize,
}

fn outcome_of(settings: &CrossSessionSettings, rows: &[ThresholdWrite]) -> &'static str {
    if rows.is_empty() {
        "no_calibration_rows"
    } else if settings.threshold_mode == "off" {
        "mode_off"
    } else if rows.iter().all(|r| r.n_labelled == 0) {
        "no_labels"
    } else {
        "wrote"
    }
}

fn n_by_source(rows: &[ThresholdWrite]) -> Value {
    let mut out = serde_json::Map::new();
    for row in rows.iter().filter(|r| r.scope == "global") {
        let key = match row.detail.get("steerer").and_then(Value::as_str) {
            Some(name) => format!("fire:{name}"),
            None => row.question.clone(),
        };
        let per_source: serde_json::Map<String, Value> = row
            .detail
            .get("by_source")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(s, v)| Some((s.clone(), v.get("n")?.clone())))
                    .collect()
            })
            .unwrap_or_default();
        out.insert(key, Value::Object(per_source));
    }
    Value::Object(out)
}

async fn execute(
    persist: &PersistHandle,
    snapshot: &CrossSessionSnapshot,
    rollup: RollupRef,
    now: DateTime<Utc>,
) -> Result<PassSummary, PersistError> {
    let mut rows = Vec::new();
    if let Some(steerer_id) = snapshot.guard_steerer_id {
        let config = crate::steerer::guard::policy::guard_config(persist, steerer_id).await;
        let modes = question_modes(persist, steerer_id).await;
        rows = plan_threshold_rows(&PassInputs {
            snapshot,
            config: &config,
            question_modes: &modes,
            now,
        });
    }
    rows.extend(plan_fire_rows(persist, snapshot, rollup, now).await?);
    let changes = persist.steerer().insert_threshold_rows(&rows).await?;
    let summary = PassSummary {
        outcome: outcome_of(&snapshot.settings, &rows),
        rows_written: rows.len(),
        changes,
    };
    persist
        .steerer()
        .record_threshold_pass_run(&ThresholdPassRunWrite {
            rollup_id: rollup.id,
            outcome: summary.outcome.to_owned(),
            threshold_mode: snapshot.settings.threshold_mode.clone(),
            rows_written: i32::try_from(rows.len()).unwrap_or(i32::MAX),
            applied: i32::try_from(summary.changes.len()).unwrap_or(i32::MAX),
            n_by_source: n_by_source(&rows),
            detail: json!({"weights": {
                "operator": snapshot.settings.weight_operator,
                "session_reply": snapshot.settings.weight_session_reply,
                "session_end": snapshot.settings.weight_session_end,
                "next_message": snapshot.settings.weight_next_message,
            }}),
        })
        .await?;
    Ok(summary)
}

pub async fn run_threshold_pass(
    persist: &PersistHandle,
    snapshot: &CrossSessionSnapshot,
    now: DateTime<Utc>,
) -> Result<PassSummary, PersistError> {
    let Some(rollup) = snapshot.rollup else {
        return Ok(PassSummary {
            changes: Vec::new(),
            outcome: "no_calibration_rows",
            rows_written: 0,
        });
    };
    match execute(persist, snapshot, rollup, now).await {
        Ok(summary) => Ok(summary),
        Err(error) => {
            let failed = ThresholdPassRunWrite {
                rollup_id: rollup.id,
                outcome: "error".to_owned(),
                threshold_mode: snapshot.settings.threshold_mode.clone(),
                rows_written: 0,
                applied: 0,
                n_by_source: json!({}),
                detail: json!({"error": error.to_string()}),
            };
            if let Err(record) = persist.steerer().record_threshold_pass_run(&failed).await {
                tracing::warn!(error = %record, rollup = rollup.id, "cross_session: pass failure not recorded");
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_session::weighting::Tally;

    fn weighted(edge_n: &[(f64, f64, f64)]) -> Weighted {
        let inputs: Vec<BinInput> = edge_n
            .iter()
            .map(|(edge, k, n)| BinInput {
                lower_edge: *edge,
                sources: [("operator".to_owned(), Tally { k: *k, n: *n })].into(),
            })
            .collect();
        weigh(&inputs, &CrossSessionSettings::default())
    }

    fn spec(rollup: RollupRef) -> RowSpec<'static> {
        RowSpec {
            steerer_id: 6,
            question: "fire",
            project: None,
            rollup,
            question_mode: "append",
            bar: 0.75,
            floor: 0.6,
            static_threshold: 0.7,
            previous: None,
            steerer_name: Some("verifier"),
            now: Utc::now(),
        }
    }

    fn rollup() -> RollupRef {
        RollupRef {
            id: 9,
            finished_at: Utc::now(),
        }
    }

    fn apply_settings() -> CrossSessionSettings {
        CrossSessionSettings {
            threshold_mode: "apply".to_owned(),
            ..CrossSessionSettings::default()
        }
    }

    #[test]
    fn build_row_with_labels_but_no_bin_clearing_the_bar_is_below_bar() {
        let row = build_row(
            &apply_settings(),
            &spec(rollup()),
            &weighted(&[(0.95, 20.0, 40.0), (0.9, 5.0, 10.0)]),
        );
        assert_eq!(row.decision, "below_bar");
        assert_eq!(row.n_labelled, 50);
        assert!((row.applied_threshold - 0.7).abs() < 1e-6);
        assert_eq!(row.label_basis, "weighted_by_source");
        assert_eq!(row.detail["steerer"], "verifier");
    }

    #[test]
    fn build_row_with_too_few_labels_is_insufficient() {
        let row = build_row(
            &apply_settings(),
            &spec(rollup()),
            &weighted(&[(0.95, 3.0, 3.0)]),
        );
        assert_eq!(row.decision, "insufficient");
        assert_eq!(row.candidate_threshold, None);
    }

    #[test]
    fn build_row_with_a_clearing_bin_applies_it_within_bounds() {
        let row = build_row(
            &apply_settings(),
            &spec(rollup()),
            &weighted(&[(0.95, 45.0, 45.0), (0.9, 38.0, 40.0)]),
        );
        assert_eq!(row.decision, "applied");
        assert!((row.applied_threshold - 0.9).abs() < 1e-6);
        assert_eq!(row.detail["by_source"]["operator"]["n"], 85.0);
    }

    #[test]
    fn outcome_of_names_the_reason_a_pass_wrote_nothing_useful() {
        let settings = apply_settings();
        let empty: Vec<ThresholdWrite> = Vec::new();
        assert_eq!(outcome_of(&settings, &empty), "no_calibration_rows");
        let unlabelled = build_row(&settings, &spec(rollup()), &weighted(&[]));
        assert_eq!(
            outcome_of(&settings, std::slice::from_ref(&unlabelled)),
            "no_labels"
        );
        let off = CrossSessionSettings::default();
        assert_eq!(
            outcome_of(&off, std::slice::from_ref(&unlabelled)),
            "mode_off"
        );
        let labelled = build_row(&settings, &spec(rollup()), &weighted(&[(0.95, 3.0, 3.0)]));
        assert_eq!(outcome_of(&settings, &[labelled]), "wrote");
    }

    #[test]
    fn n_by_source_keys_fire_rows_by_steerer_and_counts_global_rows_only() {
        let settings = apply_settings();
        let row = build_row(&settings, &spec(rollup()), &weighted(&[(0.95, 3.0, 3.0)]));
        let mut project = row.clone();
        project.scope = "project".to_owned();
        let counted = n_by_source(&[row, project]);
        assert_eq!(counted["fire:verifier"]["operator"], 3.0);
        assert_eq!(counted.as_object().map(serde_json::Map::len), Some(1));
    }
}
