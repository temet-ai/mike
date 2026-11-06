use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::{Value, json};

use super::assets::{Assets, GtRow};
use super::strata::{Stratum, SyntheticReason, era_of, is_real};

pub const FLEET_RUN: &str = "fleet-mirror-7772e9a0";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeedRow {
    pub subject_kind: &'static str,
    pub subject_id: i64,
    pub axis: &'static str,
    pub label: String,
    pub method: &'static str,
    pub tags: Vec<String>,
    pub severity: Option<String>,
    pub confidence: Option<String>,
    pub evidence: Option<String>,
    pub source: &'static str,
    pub provenance: Value,
    pub inclusion_prob: Option<f64>,
}

fn lower(label: &str) -> String {
    label.to_lowercase()
}

fn severity(value: Option<&String>) -> Option<String> {
    value
        .map(|s| s.to_lowercase())
        .filter(|s| matches!(s.as_str(), "none" | "minor" | "major" | "severe"))
}

fn base_provenance(
    asset: &str,
    row_index: usize,
    extra: Value,
    controls: &[i64],
    turn_id: i64,
) -> Value {
    let mut value = json!({"fleet_run": FLEET_RUN, "asset": asset, "row_index": row_index});
    if let (Some(map), Value::Object(more)) = (value.as_object_mut(), extra) {
        for (key, v) in more {
            map.insert(key, v);
        }
        if controls.contains(&turn_id) {
            map.insert("control".to_owned(), json!(true));
        }
    }
    value
}

pub struct GtContext<'a> {
    pub strata: &'a HashMap<i64, Stratum>,
    pub stratum_sizes: &'a std::collections::BTreeMap<Stratum, usize>,
    pub real: &'a HashSet<i64>,
    pub controls: &'a [i64],
}

fn gt_inclusion(row: &GtRow, ctx: &GtContext<'_>) -> Option<f64> {
    if !ctx.real.contains(&row.turn_id) {
        return None;
    }
    let stratum = ctx.strata.get(&row.turn_id)?;
    let size = *ctx.stratum_sizes.get(stratum)?;
    (size > 0).then(|| (25.0 / size as f64).min(1.0))
}

fn gt_delivery(row: &GtRow) -> String {
    let raw = row.final_label.delivery.to_lowercase();
    if raw == "delivered" && row.final_label.irreversible_action {
        "shipped".to_owned()
    } else {
        raw
    }
}

pub fn ground_truth_rows(assets: &Assets, ctx: &GtContext<'_>) -> Vec<SeedRow> {
    let mut out = Vec::new();
    for (index, row) in assets.ground_truth.rows.iter().enumerate() {
        let f = &row.final_label;
        let mut extra = Value::Object(row.extra.clone());
        if let Some(map) = extra.as_object_mut() {
            map.insert("final_extra".to_owned(), Value::Object(f.extra.clone()));
            map.insert("supplement".to_owned(), json!(row.supplement));
            map.insert("raw_delivery".to_owned(), json!(f.delivery));
            map.insert("real".to_owned(), json!(ctx.real.contains(&row.turn_id)));
        }
        let provenance = base_provenance("ground_truth", index, extra, ctx.controls, row.turn_id);
        let inclusion = gt_inclusion(row, ctx);
        let axes: [(&'static str, String); 5] = [
            ("reaction", lower(&f.label)),
            ("delivery", gt_delivery(row)),
            (
                "fact_own_prompt_corrects_prior",
                f.own_prompt_corrects_prior.to_string(),
            ),
            (
                "fact_assistant_self_correction",
                f.assistant_self_correction.to_string(),
            ),
            (
                "fact_irreversible_action",
                f.irreversible_action.to_string(),
            ),
        ];
        for (axis, label) in axes {
            out.push(SeedRow {
                subject_kind: "turn",
                subject_id: row.turn_id,
                axis,
                label,
                method: "double_blind_adjudicated",
                tags: f.failure_tags.clone(),
                severity: severity(f.severity.as_ref()),
                confidence: f.confidence.clone(),
                evidence: f.evidence.clone(),
                source: "fleet_mirror/ground_truth",
                provenance: provenance.clone(),
                inclusion_prob: inclusion,
            });
        }
    }
    out
}

pub fn session_end_rows(assets: &Assets, controls: &[i64]) -> Vec<SeedRow> {
    let se = &assets.session_ends;
    let mut out: Vec<SeedRow> = se
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| SeedRow {
            subject_kind: "session_end",
            subject_id: row.turn_id,
            axis: "session_end",
            label: lower(&row.final_label.label),
            method: "double_blind_adjudicated",
            tags: row.final_label.failure_tags.clone(),
            severity: severity(row.final_label.severity.as_ref()),
            confidence: row.final_label.confidence.clone(),
            evidence: row.final_label.evidence.clone(),
            source: "fleet_mirror/session_ends",
            provenance: base_provenance(
                "session_ends",
                index,
                json!({"row": row.extra, "kappa_label": se.kappa_label}),
                controls,
                row.turn_id,
            ),
            inclusion_prob: Some(1.0),
        })
        .collect();
    for (index, mapped) in se.mapping.mapped.iter().enumerate() {
        for pattern in &mapped.patterns {
            out.push(pattern_row(
                mapped.turn_id,
                pattern,
                mapped.note.clone(),
                index,
                "mapped",
            ));
        }
    }
    for (index, pattern) in se.mapping.new_patterns.iter().enumerate() {
        for turn_id in &pattern.turn_ids {
            out.push(pattern_row(
                *turn_id,
                &pattern.name,
                Some(pattern.definition.clone()),
                index,
                "new_patterns",
            ));
        }
    }
    out
}

fn pattern_row(
    turn_id: i64,
    pattern: &str,
    evidence: Option<String>,
    index: usize,
    part: &str,
) -> SeedRow {
    SeedRow {
        subject_kind: "session_end",
        subject_id: turn_id,
        axis: "failure_tag",
        label: pattern.to_owned(),
        method: "double_blind_adjudicated",
        tags: Vec::new(),
        severity: None,
        confidence: None,
        evidence,
        source: "fleet_mirror/session_ends",
        provenance: json!({"fleet_run": FLEET_RUN, "asset": "session_ends", "mapping": part, "row_index": index}),
        inclusion_prob: Some(1.0),
    }
}

pub fn census_rows(assets: &Assets, controls: &[i64]) -> Vec<SeedRow> {
    assets
        .census
        .iter()
        .enumerate()
        .map(|(index, row)| SeedRow {
            subject_kind: "turn",
            subject_id: row.turn_id,
            axis: "reaction",
            label: lower(&row.label),
            method: "single_model",
            tags: row.failure_tags.clone(),
            severity: severity(row.severity.as_ref()),
            confidence: row.confidence.clone(),
            evidence: row.evidence.clone(),
            source: "fleet_mirror/census",
            provenance: base_provenance(
                "census",
                index,
                Value::Object(row.extra.clone()),
                controls,
                row.turn_id,
            ),
            inclusion_prob: Some(1.0),
        })
        .collect()
}

type AuditedQuotes<'a> = HashMap<(i64, &'a str), (&'a Option<String>, &'a Option<String>)>;

pub fn mirror_rows(assets: &Assets) -> Vec<SeedRow> {
    let audited: AuditedQuotes<'_> = assets
        .mirror
        .results
        .iter()
        .flat_map(|r| {
            r.audit.confirmed.iter().map(move |a| {
                (
                    (a.turn_id, r.p.name.as_str()),
                    (&a.operator_quote, &a.assistant_behaviour),
                )
            })
        })
        .collect();
    let mut out = Vec::new();
    for (index, tagged) in assets.mirror.tagged.iter().enumerate() {
        let patterns: Vec<&str> = if tagged.patterns.is_empty() {
            vec!["none"]
        } else {
            tagged.patterns.iter().map(String::as_str).collect()
        };
        for pattern in patterns {
            let mut provenance =
                json!({"fleet_run": FLEET_RUN, "asset": "mirror", "row_index": index});
            if let (Some(map), Some((quote, behaviour))) = (
                provenance.as_object_mut(),
                audited.get(&(tagged.id, pattern)),
            ) {
                map.insert("audited".to_owned(), json!(true));
                map.insert("operator_quote".to_owned(), json!(quote));
                map.insert("assistant_behaviour".to_owned(), json!(behaviour));
            }
            out.push(SeedRow {
                subject_kind: "turn",
                subject_id: tagged.id,
                axis: "failure_tag",
                label: pattern.to_owned(),
                method: "single_model",
                tags: Vec::new(),
                severity: None,
                confidence: None,
                evidence: None,
                source: "fleet_mirror/mirror",
                provenance,
                inclusion_prob: Some(1.0),
            });
        }
    }
    out
}

pub fn strata_of(assets: &Assets) -> HashMap<i64, Stratum> {
    assets
        .turns
        .iter()
        .map(|t| {
            (
                t.turn_id,
                (
                    era_of(&t.created_at),
                    t.stored_outcome.clone().unwrap_or_default(),
                ),
            )
        })
        .collect()
}

pub fn real_turns(assets: &Assets, synthetic: &HashMap<i64, SyntheticReason>) -> HashSet<i64> {
    assets
        .turns
        .iter()
        .filter(|t| is_real(t, synthetic))
        .map(|t| t.turn_id)
        .collect()
}
