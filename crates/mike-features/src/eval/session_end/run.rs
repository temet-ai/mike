use crate::store::persistence::eval_repo::EvalPersist as _;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::score::{EndItem, score_session_end_run};
use crate::dossier::render::RENDERER_VERSION;
use crate::dossier::view::types::{AsOf, AsOfMode};
use crate::judge_worker::session_end::config::SessionJudgeV3Config;
use crate::judge_worker::session_end::live::verdict_row;
use crate::judge_worker::session_end::run::{
    SessionJudgeFailure, SessionJudgementV3, SessionSubject, judge_session_v3,
};
use crate::judge_worker::session_end::{EndRule, SessionEndLabel};
use crate::llm::{LlmCallContext, OpenRouterClient};
use crate::store::persistence::eval_runs::NewEvalRun;
use crate::store::persistence::session_end::{NewSessionEndItem, SessionEndLabelItem};
use crate::store::persistence::{PersistError, PersistHandle};

const ITEM_ESTIMATE_USD: f64 = 0.01;
const SCORE_SEED: u64 = 13;

#[derive(Debug, Clone)]
pub struct SessionEndRunRequest {
    pub bundle_id: i64,
    pub cfg: SessionJudgeV3Config,
    pub split: String,
    pub tiers: Vec<String>,
    pub labels_as_of: DateTime<Utc>,
    pub purpose: String,
    pub repeat_of: Option<i64>,
    pub max_usd: f64,
    pub limit: Option<usize>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionEndRunOutcome {
    DryRun {
        items: usize,
        labels: BTreeMap<String, usize>,
        estimate_usd: f64,
    },
    Finished {
        run_id: i64,
        status: String,
        report: Value,
    },
}

pub fn session_end_label_hash(items: &[SessionEndLabelItem]) -> String {
    let mut ids: Vec<i64> = items.iter().map(|i| i.label_id).collect();
    ids.sort_unstable();
    let text = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn label_counts(items: &[SessionEndLabelItem]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for i in items {
        *counts.entry(i.label.clone()).or_insert(0) += 1;
    }
    counts
}

struct ItemResult {
    outcome: &'static str,
    verdict_id: Option<i64>,
    llm_end_label: Option<&'static str>,
    rule_fired: Option<&'static str>,
    effective: Option<&'static str>,
    cost_usd: f64,
}

impl ItemResult {
    fn unjudged(outcome: &'static str) -> Self {
        Self {
            outcome,
            verdict_id: None,
            llm_end_label: None,
            rule_fired: None,
            effective: None,
            cost_usd: 0.0,
        }
    }
}

async fn judged(
    persist: &PersistHandle,
    req: &SessionEndRunRequest,
    item: &SessionEndLabelItem,
    j: &SessionJudgementV3,
) -> ItemResult {
    let row = verdict_row(j, &req.cfg, item.session_id, "eval", Some(req.bundle_id));
    let verdict_id = match persist.eval().insert_session_verdict_v3(&row).await {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(end_turn_id = item.end_turn_id, error = %e, "session-end eval: verdict insert failed");
            None
        }
    };
    let llm_end_label = SessionEndLabel::parse(&j.verdict.end_label).map(SessionEndLabel::code);
    let outcome = if verdict_id.is_some() || llm_end_label.is_some() {
        "judged"
    } else {
        "failed"
    };
    ItemResult {
        outcome,
        verdict_id,
        llm_end_label,
        rule_fired: j.rule_fired.map(EndRule::code),
        effective: j.end.as_ref().map(|e| e.label.code()),
        cost_usd: j.usage.cost_usd.unwrap_or(0.0),
    }
}

async fn judge_item(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    req: &SessionEndRunRequest,
    item: &SessionEndLabelItem,
) -> ItemResult {
    let llm = llm.with_call_context(LlmCallContext {
        session_id: Some(item.session_id),
        steerer_id: None,
        operator_turn_id: Some(item.end_turn_id),
    });
    let subject = SessionSubject {
        session_db_id: item.session_id,
        as_of: AsOf {
            at: item.closed_at.unwrap_or_else(Utc::now),
            mode: AsOfMode::Reconstructed,
        },
    };
    match judge_session_v3(persist, &llm, &req.cfg, &subject).await {
        Ok(j) => judged(persist, req, item, &j).await,
        Err(SessionJudgeFailure::Refused) => ItemResult::unjudged("refused"),
        Err(e) => {
            tracing::warn!(end_turn_id = item.end_turn_id, error = %e, "session-end eval: judging failed");
            ItemResult::unjudged("failed")
        }
    }
}

async fn create_run(
    persist: &PersistHandle,
    req: &SessionEndRunRequest,
    items: &[SessionEndLabelItem],
) -> Result<i64, PersistError> {
    let argv = json!({
        "role": "session_judge",
        "split": req.split,
        "tiers": req.tiers,
        "labels_as_of": req.labels_as_of,
        "limit": req.limit,
    });
    persist
        .eval()
        .create_eval_run(&NewEvalRun {
            purpose: &req.purpose,
            bundle_id: req.bundle_id,
            rubric_version: req.cfg.rubric_id,
            input_builder_version: RENDERER_VERSION,
            slices: &[],
            model: &req.cfg.model,
            split: &req.split,
            tiers: &req.tiers,
            holdout_batch: None,
            labels_as_of: req.labels_as_of,
            label_count: i32::try_from(items.len()).unwrap_or(i32::MAX),
            label_ids_hash: &session_end_label_hash(items),
            repeat_of_run_id: req.repeat_of,
            max_usd: req.max_usd,
            argv,
        })
        .await
}

pub async fn estimate_usd(persist: &PersistHandle, bundle_id: i64, items: usize) -> f64 {
    let mean = persist
        .eval()
        .session_v3_mean_cost(bundle_id)
        .await
        .ok()
        .flatten()
        .unwrap_or(ITEM_ESTIMATE_USD);
    mean * items as f64
}

pub async fn run_session_end_eval(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    req: &SessionEndRunRequest,
) -> Result<SessionEndRunOutcome, PersistError> {
    let mut items = persist
        .eval()
        .load_session_end_label_set(req.labels_as_of, &req.split, &req.tiers)
        .await?;
    if let Some(limit) = req.limit {
        items = items.into_iter().take(limit).collect();
    }
    if req.dry_run {
        return Ok(SessionEndRunOutcome::DryRun {
            items: items.len(),
            labels: label_counts(&items),
            estimate_usd: estimate_usd(persist, req.bundle_id, items.len()).await,
        });
    }
    let run_id = create_run(persist, req, &items).await?;
    let (status, scored) = judge_all(persist, llm, req, run_id, &items).await?;
    let report = score_session_end_run(&scored, SCORE_SEED);
    persist
        .eval()
        .refresh_session_end_run(run_id, status, Some(&report))
        .await?;
    Ok(SessionEndRunOutcome::Finished {
        run_id,
        status: status.to_owned(),
        report,
    })
}

async fn judge_all(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    req: &SessionEndRunRequest,
    run_id: i64,
    items: &[SessionEndLabelItem],
) -> Result<(&'static str, Vec<EndItem>), PersistError> {
    let mut spent = 0.0;
    let mut scored = Vec::with_capacity(items.len());
    for item in items {
        if spent >= req.max_usd {
            return Ok(("paused_budget", scored));
        }
        let r = judge_item(persist, llm, req, item).await;
        spent += r.cost_usd;
        persist
            .eval()
            .insert_session_end_item(&NewSessionEndItem {
                run_id,
                end_turn_id: item.end_turn_id,
                session_id: item.session_id,
                label_id: item.label_id,
                verdict_id: r.verdict_id,
                outcome: r.outcome,
                label: &item.label,
                llm_end_label: r.llm_end_label,
                rule_fired: r.rule_fired,
                effective_end_label: r.effective,
            })
            .await?;
        if r.outcome == "judged" {
            scored.push(EndItem {
                gold: item.label.clone(),
                llm: r.llm_end_label.map(str::to_owned),
                rule_fired: r.rule_fired.map(str::to_owned),
            });
        }
    }
    Ok(("complete", scored))
}

pub fn end_items_from_rows(
    rows: &[crate::store::persistence::session_end::SessionEndItemRow],
) -> Vec<EndItem> {
    rows.iter()
        .filter(|r| r.outcome == "judged")
        .map(|r| EndItem {
            gold: r.label.clone(),
            llm: r.llm_end_label.clone(),
            rule_fired: r.rule_fired.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(id: i64, value: &str) -> SessionEndLabelItem {
        SessionEndLabelItem {
            end_turn_id: id * 10,
            session_id: id,
            label_id: id,
            label: value.to_owned(),
            tier: "seed".to_owned(),
            closed_at: None,
            session_uuid: format!("s-{id}"),
        }
    }

    #[test]
    fn session_end_label_hash_ignores_order() {
        let a = [label(1, "done"), label(2, "unclear")];
        let b = [label(2, "unclear"), label(1, "done")];
        assert_eq!(session_end_label_hash(&a), session_end_label_hash(&b));
        assert_eq!(label_counts(&a).get("done"), Some(&1));
    }
}
