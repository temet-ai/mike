use crate::store::persistence::eval_repo::EvalPersist as _;
use anyhow::Result;
use chrono::Duration;
use serde_json::{Value, json};

use super::args_runs::EvalSessionEndDefaultsArgs;
use crate::config::CliOverrides;
use crate::eval::session_end::score::{EndItem, rule_label, rule_precision, wilson_rate};
use crate::judge_worker::session_end::defaults::{done_v1, fleet_seat_v1};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_end::SessionEndLabelItem;

const SETTLE_MINUTES: i64 = 30;
const RULES: [&str; 2] = ["fleet-seat-v1", "done-v1"];
const FRUSTRATION: &str = "closed_in_frustration";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DefaultsItem {
    pub(crate) gold: String,
    pub(crate) fleet_seat: bool,
    pub(crate) done: bool,
    pub(crate) facts_missing: bool,
}

fn as_end_items(items: &[DefaultsItem], rule: &str) -> Vec<EndItem> {
    items
        .iter()
        .map(|i| {
            let fired = match rule {
                "fleet-seat-v1" => i.fleet_seat,
                _ => i.done,
            };
            EndItem {
                gold: i.gold.clone(),
                llm: None,
                rule_fired: fired.then(|| rule.to_owned()),
            }
        })
        .collect()
}

fn rule_table(items: &[DefaultsItem], rule: &str) -> Value {
    let end_items = as_end_items(items, rule);
    let (precision, frustration_among_fires) = rule_precision(&end_items, rule);
    let target = rule_label(rule).unwrap_or_default();
    let targets = items.iter().filter(|i| i.gold == target).count() as i64;
    let recall = wilson_rate(precision.k, targets);
    let lost = end_items
        .iter()
        .filter(|i| i.rule_fired.is_some() && i.gold == FRUSTRATION)
        .count();
    json!({
        "rule": rule,
        "target_label": target,
        "fires": precision.n,
        "precision": precision,
        "recall": recall,
        "frustration_among_fires": frustration_among_fires,
        "frustration_closes_lost": lost,
    })
}

pub(crate) fn defaults_report(items: &[DefaultsItem]) -> Value {
    let mut by_label = std::collections::BTreeMap::new();
    for i in items {
        *by_label.entry(i.gold.clone()).or_insert(0usize) += 1;
    }
    json!({
        "n": items.len(),
        "facts_missing": items.iter().filter(|i| i.facts_missing).count(),
        "labels": by_label,
        "rules": RULES.iter().map(|r| rule_table(items, r)).collect::<Vec<_>>(),
    })
}

async fn evaluate(persist: &PersistHandle, label: &SessionEndLabelItem) -> Result<DefaultsItem> {
    let at = label
        .closed_at
        .map_or_else(chrono::Utc::now, |t| t + Duration::minutes(SETTLE_MINUTES));
    let facts = persist
        .eval()
        .fetch_session_end_facts(&[label.session_id], at)
        .await?
        .into_iter()
        .find(|f| f.end_turn_id == label.end_turn_id);
    Ok(DefaultsItem {
        gold: label.label.clone(),
        fleet_seat: facts.as_ref().and_then(fleet_seat_v1).is_some(),
        done: facts.as_ref().and_then(done_v1).is_some(),
        facts_missing: facts.is_none(),
    })
}

fn print_report(report: &Value) {
    println!(
        "{} labelled ending turn(s), labels {}",
        report["n"], report["labels"]
    );
    let empty = Vec::new();
    for rule in report["rules"].as_array().unwrap_or(&empty) {
        println!(
            "{}: fires {}, precision {}, recall {}, frustration among fires {}, frustration closes lost {}",
            rule["rule"],
            rule["fires"],
            rule["precision"],
            rule["recall"],
            rule["frustration_among_fires"],
            rule["frustration_closes_lost"]
        );
    }
}

pub(super) async fn session_end_defaults(
    args: EvalSessionEndDefaultsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let persist = super::connect(cli, false).await?;
    let source = Some(args.source.as_str()).filter(|s| !s.is_empty());
    let labels = persist
        .eval()
        .load_session_end_labels_by_source(chrono::Utc::now(), &args.tiers, source)
        .await?;
    let mut items = Vec::with_capacity(labels.len());
    for label in &labels {
        items.push(evaluate(&persist, label).await?);
    }
    let report = defaults_report(&items);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(gold: &str, fleet_seat: bool, done: bool) -> DefaultsItem {
        DefaultsItem {
            gold: gold.to_owned(),
            fleet_seat,
            done,
            facts_missing: false,
        }
    }

    #[test]
    fn eval_session_end_defaults_reports_precision_recall_and_n() {
        let mut items: Vec<DefaultsItem> =
            (0..13).map(|_| item("fleet_seat", true, false)).collect();
        items.push(item("fleet_seat", false, false));
        items.push(item("done", false, true));
        items.push(item("closed_in_frustration", false, true));
        let report = defaults_report(&items);
        assert_eq!(report["n"], 16);
        let fleet = &report["rules"][0];
        assert_eq!(fleet["fires"], 13);
        assert_eq!(fleet["precision"]["k"], 13);
        assert_eq!(fleet["recall"]["n"], 14);
        let done = &report["rules"][1];
        assert_eq!(done["fires"], 2);
        assert_eq!(done["frustration_closes_lost"], 1);
    }
}
