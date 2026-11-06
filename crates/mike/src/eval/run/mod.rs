pub(crate) mod bundle;
pub(crate) mod items;
pub(crate) mod labels;
#[cfg(test)]
mod runner_tests;
pub(crate) mod spend_guard;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::eval::score::{ScoreItem, score_run};
use crate::llm::OpenRouterClient;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_runs::{
    EvalRunItemRow, LabelSetFilter, LabelSetItem, NewEvalRun, RunItemWrite,
};
use items::{ItemJob, ItemResult, RunJudge, judge_item};
use labels::{LoadedSet, stratum_counts};
use spend_guard::{BudgetPause, EvalSpendGuard, SpendLimits};

const CONTEXT_BATCH: usize = 10;
const V2_ITEM_ESTIMATE_USD: f64 = 0.002;
const R3_ITEM_ESTIMATE_USD: f64 = 0.0015;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LabelOptions {
    pub(crate) split: String,
    pub(crate) tiers: Vec<String>,
    pub(crate) methods: Vec<String>,
    pub(crate) holdout_batch: Option<String>,
    pub(crate) labels_as_of: DateTime<Utc>,
    pub(crate) limit: Option<usize>,
}

impl LabelOptions {
    pub(crate) fn to_argv(&self) -> Value {
        json!({
            "split": self.split,
            "tiers": self.tiers,
            "methods": self.methods,
            "holdout_batch": self.holdout_batch,
            "labels_as_of": self.labels_as_of,
            "limit": self.limit,
        })
    }

    pub(crate) fn from_argv(argv: &Value) -> Option<Self> {
        let strings = |key: &str| -> Option<Vec<String>> {
            Some(
                argv.get(key)?
                    .as_array()?
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
            )
        };
        Some(Self {
            split: argv.get("split")?.as_str()?.to_owned(),
            tiers: strings("tiers")?,
            methods: strings("methods")?,
            holdout_batch: argv
                .get("holdout_batch")
                .and_then(Value::as_str)
                .map(str::to_owned),
            labels_as_of: argv.get("labels_as_of")?.as_str()?.parse().ok()?,
            limit: argv
                .get("limit")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok()),
        })
    }

    fn filter(&self) -> LabelSetFilter {
        LabelSetFilter {
            labels_as_of: self.labels_as_of,
            split: self.split.clone(),
            tiers: self.tiers.clone(),
            methods: self.methods.clone(),
            holdout_batch: self.holdout_batch.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StartRequest {
    pub(crate) bundle_id: Option<i64>,
    pub(crate) judge: RunJudge,
    pub(crate) labels: LabelOptions,
    pub(crate) purpose: String,
    pub(crate) repeat_of: Option<i64>,
    pub(crate) max_usd: f64,
    pub(crate) expected_hash: Option<String>,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RunOutcome {
    DryRun {
        items: usize,
        strata: Value,
        estimate_usd: f64,
    },
    Finished {
        run_id: i64,
        status: String,
        report: Option<Value>,
        pause: Option<Value>,
    },
    Refused(String),
}

pub(crate) struct Runner<'a> {
    pub(crate) persist: &'a PersistHandle,
    pub(crate) llm: &'a OpenRouterClient,
    pub(crate) limits: SpendLimits,
}

fn estimate(judge: &RunJudge, items: usize) -> f64 {
    let per = if judge.is_r3() {
        R3_ITEM_ESTIMATE_USD
    } else {
        V2_ITEM_ESTIMATE_USD
    };
    per * items as f64
}

fn pred_reaction(row: &EvalRunItemRow) -> Option<String> {
    row.verdict_reaction
        .clone()
        .or_else(|| row.verdict_legacy_label.clone())
}

pub(crate) fn score_items(rows: &[EvalRunItemRow]) -> Vec<ScoreItem> {
    rows.iter()
        .filter(|r| r.outcome != "skipped_unresolved")
        .map(|r| ScoreItem {
            turn_id: r.operator_turn_id,
            gold_reaction: r.gold_reaction.clone().unwrap_or_default(),
            gold_delivery: r.gold_delivery.clone(),
            pred_reaction: pred_reaction(r),
            pred_delivery: r.verdict_delivery.clone(),
            stratum: r.stratum.clone(),
            inclusion_prob: r.inclusion_prob,
        })
        .collect()
}

impl Runner<'_> {
    async fn resolve_bundle(&self, judge: &RunJudge) -> anyhow::Result<i64> {
        let components = judge.components();
        if let Some(id) = self
            .persist
            .find_resolver_bundle("turn_judge", &components)
            .await?
        {
            return Ok(id);
        }
        Ok(self
            .persist
            .create_candidate_bundle("turn_judge", "", &components)
            .await?)
    }

    pub(crate) async fn start(&self, req: StartRequest) -> anyhow::Result<RunOutcome> {
        if let Some(bundle_id) = req.bundle_id {
            let frozen = bundle::load_bundle_judge(self.persist, bundle_id).await?;
            anyhow::ensure!(
                frozen == req.judge,
                "judge configuration differs from selected bundle {bundle_id}"
            );
        }
        let loaded = labels::load(self.persist, req.labels.filter(), req.labels.limit).await?;
        if let Some(expected) = req.expected_hash.as_deref()
            && expected != loaded.label_ids_hash
        {
            return Ok(RunOutcome::Refused(
                "the label set changed since the reference run (label_ids_hash differs)".to_owned(),
            ));
        }
        if req.dry_run {
            return Ok(RunOutcome::DryRun {
                items: loaded.items.len(),
                strata: json!(stratum_counts(&loaded.items)),
                estimate_usd: estimate(&req.judge, loaded.items.len()),
            });
        }
        let bundle_id = match req.bundle_id {
            Some(bundle_id) => bundle_id,
            None => self.resolve_bundle(&req.judge).await?,
        };
        let run_id = self.create_run(&req, &loaded, bundle_id).await?;
        let guard = EvalSpendGuard::new(self.limits, 0.0, 0);
        self.execute(run_id, &req.judge, bundle_id, &loaded.items, guard)
            .await
    }

    async fn create_run(
        &self,
        req: &StartRequest,
        loaded: &LoadedSet,
        bundle_id: i64,
    ) -> anyhow::Result<i64> {
        let run = NewEvalRun {
            purpose: &req.purpose,
            bundle_id,
            rubric_version: req.judge.rubric_id(),
            input_builder_version: req.judge.builder(),
            model: req.judge.model(),
            split: &req.labels.split,
            tiers: &req.labels.tiers,
            holdout_batch: req.labels.holdout_batch.as_deref(),
            labels_as_of: loaded.labels_as_of,
            label_count: i32::try_from(loaded.items.len()).unwrap_or(i32::MAX),
            label_ids_hash: &loaded.label_ids_hash,
            repeat_of_run_id: req.repeat_of,
            max_usd: req.max_usd,
            argv: req.labels.to_argv(),
        };
        Ok(self.persist.create_eval_run(&run).await?)
    }

    pub(crate) async fn resume(&self, run_id: i64, judge: &RunJudge) -> anyhow::Result<RunOutcome> {
        let Some(run) = self.persist.get_eval_run(run_id).await? else {
            return Ok(RunOutcome::Refused(format!("no eval run with id {run_id}")));
        };
        let Some(options) = LabelOptions::from_argv(&run.argv) else {
            return Ok(RunOutcome::Refused(format!(
                "run {run_id} has no stored label options"
            )));
        };
        let loaded = labels::load(self.persist, options.filter(), options.limit).await?;
        if loaded.label_ids_hash != run.label_ids_hash {
            return Ok(RunOutcome::Refused(format!(
                "run {run_id}'s label set changed; it cannot be resumed"
            )));
        }
        let done: Vec<i64> = self
            .persist
            .all_eval_run_items(run_id)
            .await?
            .into_iter()
            .filter(|i| i.outcome == "judged" || i.outcome == "skipped_unresolved")
            .map(|i| i.operator_turn_id)
            .collect();
        let pending: Vec<LabelSetItem> = loaded
            .items
            .into_iter()
            .filter(|i| !done.contains(&i.operator_turn_id))
            .collect();
        let judged = u32::try_from(run.items_judged).unwrap_or(0);
        let guard = EvalSpendGuard::new(
            SpendLimits {
                run_max_usd: run.max_usd,
                ..self.limits
            },
            run.cost_usd,
            judged,
        );
        self.execute(run_id, judge, run.bundle_id, &pending, guard)
            .await
    }

    async fn record(
        &self,
        run_id: i64,
        label: &LabelSetItem,
        result: &ItemResult,
    ) -> anyhow::Result<()> {
        let (verdict_id, outcome, reaction_correct, delivery_correct) = match result {
            ItemResult::Judged {
                verdict_id,
                reaction_correct,
                delivery_correct,
                ..
            } => (
                Some(*verdict_id),
                "judged",
                Some(*reaction_correct),
                *delivery_correct,
            ),
            ItemResult::Refused => (None, "refused", None, None),
            ItemResult::Failed => (None, "failed", None, None),
            ItemResult::Skipped => (None, "skipped_unresolved", None, None),
        };
        let write = RunItemWrite {
            run_id,
            operator_turn_id: label.operator_turn_id,
            reaction_label_id: Some(label.reaction_label_id),
            delivery_label_id: label.delivery_label_id,
            turn_verdict_id: verdict_id,
            outcome,
            stratum: &label.stratum,
            inclusion_prob: label.inclusion_prob,
            reaction_correct,
            delivery_correct,
        };
        Ok(self.persist.upsert_eval_run_item(&write).await?)
    }

    async fn check_budget(
        &self,
        guard: &EvalSpendGuard,
    ) -> anyhow::Result<Result<(), BudgetPause>> {
        let all = self.persist.sum_todays_llm_spend_usd().await?;
        let eval = self.persist.eval_spend_today_usd().await?;
        Ok(guard.check(all, eval))
    }

    async fn execute(
        &self,
        run_id: i64,
        judge: &RunJudge,
        bundle_id: i64,
        items: &[LabelSetItem],
        mut guard: EvalSpendGuard,
    ) -> anyhow::Result<RunOutcome> {
        for batch in items.chunks(CONTEXT_BATCH) {
            let ids: Vec<i64> = batch.iter().map(|i| i.operator_turn_id).collect();
            let rows = self.persist.fetch_judge_turn_contexts(&ids).await?;
            for label in batch {
                if let Err(pause) = self.check_budget(&guard).await? {
                    return self.pause(run_id, &pause).await;
                }
                let Some(row) = rows.iter().find(|r| r.turn_id == label.operator_turn_id) else {
                    self.record(run_id, label, &ItemResult::Skipped).await?;
                    continue;
                };
                let job = ItemJob {
                    persist: self.persist,
                    llm: self.llm,
                    judge,
                    bundle_id,
                    row,
                    label,
                };
                let result = judge_item(&job).await?;
                self.record(run_id, label, &result).await?;
                match &result {
                    ItemResult::Judged { usage, .. } => guard.record(usage),
                    ItemResult::Refused => {
                        return self
                            .pause(run_id, &json!({"reason": "governor_refused"}))
                            .await;
                    }
                    _ => {}
                }
            }
        }
        self.finish(run_id, judge).await
    }

    async fn pause(
        &self,
        run_id: i64,
        reason: &impl serde::Serialize,
    ) -> anyhow::Result<RunOutcome> {
        self.persist
            .refresh_eval_run(run_id, "paused_budget", None)
            .await?;
        Ok(RunOutcome::Finished {
            run_id,
            status: "paused_budget".to_owned(),
            report: None,
            pause: Some(serde_json::to_value(reason)?),
        })
    }

    async fn finish(&self, run_id: i64, judge: &RunJudge) -> anyhow::Result<RunOutcome> {
        let rows = self.persist.all_eval_run_items(run_id).await?;
        let seed = u64::try_from(run_id).unwrap_or(0);
        let mut report = score_run(&score_items(&rows), judge.is_r3(), seed);
        if let Some(map) = report.as_object_mut() {
            map.insert(
                "items_skipped".to_owned(),
                json!(
                    rows.iter()
                        .filter(|r| r.outcome == "skipped_unresolved")
                        .count()
                ),
            );
            map.insert(
                "items_refused".to_owned(),
                json!(rows.iter().filter(|r| r.outcome == "refused").count()),
            );
            map.insert(
                "items_failed".to_owned(),
                json!(rows.iter().filter(|r| r.outcome == "failed").count()),
            );
        }
        self.persist
            .refresh_eval_run(run_id, "complete", Some(&report))
            .await?;
        Ok(RunOutcome::Finished {
            run_id,
            status: "complete".to_owned(),
            report: Some(report),
            pause: None,
        })
    }
}
