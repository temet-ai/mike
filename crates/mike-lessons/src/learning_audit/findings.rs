use serde::Serialize;
use serde_json::{Value, json};

use super::promotion::{PromotionState, ledger_project_of};
use super::rows::{float, int, non_empty, rows, text, truthy};

const WORKER_TOGGLE_GRACE_DAYS: f64 = 2.0;
const GLOBAL: &str = "global";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct Finding {
    pub(super) kind: String,
    pub(super) project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) evidence: Option<Value>,
}

impl Finding {
    pub(super) fn new(kind: &str, project: &str, evidence: Option<Value>) -> Self {
        Self {
            kind: kind.to_owned(),
            project: project.to_owned(),
            evidence,
        }
    }

    fn global(kind: &str, evidence: Option<Value>) -> Self {
        Self::new(kind, GLOBAL, evidence)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct MemoryGrammarTotals {
    pub(super) recall_tag_requests: i64,
    pub(super) recall_injected: i64,
    pub(super) update_tags_found: i64,
    pub(super) update_lessons_written: i64,
    pub(super) update_lessons_skipped: i64,
    pub(super) harvested_lessons: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SkillLoad {
    pub(super) skill: String,
    pub(super) loads: i64,
    pub(super) sessions: i64,
    pub(super) generated: bool,
}

fn sum(data: &Value, name: &str, key: &str) -> i64 {
    rows(data, name).iter().map(|row| int(row, key)).sum()
}

pub(super) fn memory_grammar_totals(data: &Value) -> MemoryGrammarTotals {
    let recall_injected = rows(data, "memory_recall_tags")
        .iter()
        .map(|row| int(row, "injected") + int(row, "delivered_out_of_band"))
        .sum();
    MemoryGrammarTotals {
        recall_tag_requests: sum(data, "memory_recall_tags", "requests_with_recall_tags"),
        recall_injected,
        update_tags_found: sum(data, "memory_update_tags", "tags_found"),
        update_lessons_written: sum(data, "memory_update_tags", "lessons_written"),
        update_lessons_skipped: sum(data, "memory_update_tags", "lessons_skipped"),
        harvested_lessons: sum(data, "memory_harvested_lessons", "lessons"),
    }
}

pub(super) fn generated_skill_loads(data: &Value, promotion: &PromotionState) -> Vec<SkillLoad> {
    let folder_names = promotion.ledger_folder_names();
    rows(data, "skill_loads")
        .iter()
        .map(|row| {
            let skill = non_empty(row, "skill");
            let project = ledger_project_of(row);
            let generated = skill.is_some_and(|name| {
                project.is_some_and(|uuid| promotion.ledger.contains_key(&format!("{uuid}:{name}")))
                    || promotion.manifest_names.contains(name)
                    || (project.is_none() && folder_names.contains(name))
            });
            SkillLoad {
                skill: skill.unwrap_or_default().to_owned(),
                loads: int(row, "loads"),
                sessions: int(row, "sessions"),
                generated,
            }
        })
        .collect()
}

fn memory_grammar_findings(data: &Value) -> Option<Finding> {
    data.get("memory_update_tags")?;
    let totals = memory_grammar_totals(data);
    (totals.recall_tag_requests == 0 && totals.update_tags_found == 0)
        .then(|| Finding::global("memory_grammar_unused", serde_json::to_value(totals).ok()))
}

fn delivery_findings(data: &Value) -> Vec<Finding> {
    let injected: Vec<&Value> = rows(data, "injections")
        .iter()
        .filter(|row| truthy(row.get("injections")))
        .map(|row| &row["project"])
        .collect();
    let mut out = Vec::new();
    for row in rows(data, "turns") {
        let counted = row.get("genuine_turns").or_else(|| row.get("turns"));
        if truthy(counted) && !injected.contains(&&row["project"]) {
            out.push(Finding::new(
                "no_recorded_lesson_delivery",
                &text(row, "project"),
                Some(row.clone()),
            ));
        }
        if truthy(row.get("missing_provenance")) || truthy(row.get("stale_outcome_labels")) {
            out.push(Finding::new(
                "outcome_provenance_incomplete",
                &text(row, "project"),
                Some(row.clone()),
            ));
        }
    }
    out
}

fn lesson_findings(data: &Value) -> Vec<Finding> {
    rows(data, "lessons")
        .iter()
        .filter(|row| row["source"] == "operator" && truthy(row.get("pending_embeddings")))
        .map(|row| {
            Finding::new(
                "operator_lessons_unindexed",
                &text(row, "project"),
                Some(row.clone()),
            )
        })
        .collect()
}

fn skill_findings(data: &Value, promotion: &PromotionState) -> Option<Finding> {
    let loads = generated_skill_loads(data, promotion);
    if loads.is_empty() || loads.iter().any(|load| load.generated) {
        return None;
    }
    Some(Finding::global(
        "generated_skills_never_loaded",
        Some(json!({
            "total_loads": loads.iter().map(|load| load.loads).sum::<i64>(),
            "generated_loads": 0,
        })),
    ))
}

fn llm_findings(data: &Value) -> Vec<Finding> {
    let toggles = rows(data, "worker_toggles")
        .iter()
        .filter(|row| {
            row["enabled"] != Value::Bool(true)
                && float(row, "days_in_state") >= WORKER_TOGGLE_GRACE_DAYS
        })
        .map(|row| Finding::global("worker_toggle_off", Some(row.clone())));
    let roles = rows(data, "llm_roles")
        .iter()
        .filter(|row| row["last_outcome"] != "ok")
        .map(|row| Finding::global("llm_role_failing", Some(row.clone())));
    toggles.chain(roles).collect()
}

pub(super) fn live_findings(data: &Value) -> Vec<Finding> {
    rows(data, "skill_live")
        .iter()
        .filter(|row| row["retired_reason"] == "corrected_above_base")
        .map(|row| {
            Finding::new(
                "skill_retired_corrected_above_base",
                &text(row, "project"),
                Some(row.clone()),
            )
        })
        .collect()
}

pub(super) fn findings(data: &Value, promotion: &PromotionState) -> Vec<Finding> {
    let mut out: Vec<Finding> = memory_grammar_findings(data).into_iter().collect();
    out.extend(
        rows(data, "embedding_setting")
            .iter()
            .filter(|row| row["value"] == Value::Bool(false))
            .map(|row| Finding::global("embedding_disabled", Some(row.clone()))),
    );
    out.extend(delivery_findings(data));
    out.extend(lesson_findings(data));
    let measured = rows(data, "effects")
        .iter()
        .any(|row| row["family"] == "action_effect" && truthy(row.get("rows")));
    if !measured {
        out.push(Finding::global("effect_measurement_missing", None));
    }
    out.extend(skill_findings(data, promotion));
    out.extend(llm_findings(data));
    out.extend(live_findings(data));
    out
}

pub(super) fn opportunity(row: &Value) -> Value {
    let mut item = row.as_object().cloned().unwrap_or_default();
    item.insert("status".to_owned(), json!("candidate_requires_validation"));
    item.insert(
        "next_action".to_owned(),
        json!(
            "Inspect original tool input/result; match an existing recipe before creating one; validate dependencies and invalidation before reuse."
        ),
    );
    item.insert(
        "evidence_url".to_owned(),
        json!(format!(
            "http://localhost:9989/api/requests/{}/tool-calls",
            text(row, "example_request_id")
        )),
    );
    item.insert(
        "savings_status".to_owned(),
        json!("Repeated bytes are observed, not proven avoidable tokens."),
    );
    Value::Object(item)
}
