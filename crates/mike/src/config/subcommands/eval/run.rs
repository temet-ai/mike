use std::sync::Arc;

use anyhow::{Context, Result, bail};
use arc_swap::ArcSwap;
use tokio_util::task::TaskTracker;

use super::args_runs::EvalRunArgs;
use crate::analysis::judge::input_v2::{INPUT_BUILDER_V2, builder_for};
use crate::analysis::judge::rubric::{RubricFamily, effective_rubric, latest, rubric};
use crate::config::CliOverrides;
use crate::config::persist_bootstrap::PersistLlmUsageSink;
use crate::eval::run::bundle::load_bundle_judge;
use crate::eval::run::items::{RunJudge, judge_from_components};
use crate::eval::run::spend_guard::{SharePcts, SpendLimits, eval_allowance_usd};
use crate::eval::run::{LabelOptions, RunOutcome, Runner, StartRequest};
use crate::judge_worker::v2_judge::V2JudgeConfig;
use crate::llm::{LlmRole, LlmUsageSink, OpenRouterClient};
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;

pub(super) const EXIT_PAUSED: i32 = 3;
pub(super) const EXIT_REFUSED: i32 = 4;

pub(super) fn api_key(
    config: &crate::config::ProxyConfig,
    overlay: &SettingsOverlay,
    model: &str,
) -> Result<String> {
    crate::llm::key::cli_api_key(
        config.steerer.openrouter_api_key.as_ref(),
        &overlay.llm_provider,
        None,
        model,
    )
    .map_err(|e| anyhow::anyhow!("mike eval run: {e}"))
}

pub(super) fn share_pcts(o: &SettingsOverlay) -> SharePcts {
    let pct = |v: i64| u32::try_from(v).unwrap_or(0);
    SharePcts {
        eval: pct(o.llm_budget_eval_share_pct),
        total: [
            o.llm_budget_steerer_share_pct,
            o.llm_budget_session_judge_share_pct,
            o.llm_budget_judge_worker_share_pct,
            o.llm_budget_lesson_relations_share_pct,
            o.llm_budget_eval_share_pct,
            o.llm_budget_other_share_pct,
        ]
        .into_iter()
        .map(pct)
        .sum(),
    }
}

pub(super) fn install_governor(overlay: &SettingsOverlay, sink: &Arc<dyn LlmUsageSink>) {
    let mut o = overlay.clone();
    o.llm_budget_steerer_share_pct = 0;
    o.llm_budget_session_judge_share_pct = 0;
    o.llm_budget_judge_worker_share_pct = 0;
    o.llm_budget_lesson_relations_share_pct = 0;
    o.llm_budget_other_share_pct = 0;
    o.llm_budget_eval_share_pct = 100;
    crate::llm::limiter::install_runtime(Arc::new(ArcSwap::from_pointee(o)), Some(sink.clone()));
}

fn judge_model(
    args: &EvalRunArgs,
    overlay: &SettingsOverlay,
    config: &crate::config::ProxyConfig,
) -> String {
    args.model
        .clone()
        .or_else(|| overlay.judge_worker_model.clone().filter(|m| !m.is_empty()))
        .or_else(|| config.steerer.judge.model.clone().filter(|m| !m.is_empty()))
        .unwrap_or_else(|| config.steerer.model.clone())
}

fn build_judge(
    args: &EvalRunArgs,
    overlay: &SettingsOverlay,
    config: &crate::config::ProxyConfig,
) -> Result<RunJudge> {
    if !args.slices.is_empty() {
        bail!(
            "no history slice is approved; --slices {:?} is refused",
            args.slices
        );
    }
    let base = match args.rubric.as_deref() {
        Some(id) => rubric(id).with_context(|| format!("unknown rubric {id:?}"))?,
        None => latest(RubricFamily::TwoAxis),
    };
    let override_text = args
        .prompt_file
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()?;
    let effective = effective_rubric(base, override_text.as_deref());
    let llm_settings = crate::settings::resolve_judge_worker_llm_settings(overlay);
    let model = judge_model(args, overlay, config);
    if base.family == RubricFamily::LegacyOutcome {
        return Ok(RunJudge::R3 {
            rubric: effective,
            model,
            llm_settings,
        });
    }
    let builder = builder_for(&args.builder).unwrap_or(INPUT_BUILDER_V2);
    Ok(RunJudge::V2(V2JudgeConfig {
        rubric: effective,
        builder,
        model,
        llm_settings,
    }))
}

async fn label_options(
    persist: &PersistHandle,
    args: &EvalRunArgs,
) -> Result<(LabelOptions, Option<String>)> {
    let mut options = LabelOptions {
        split: args.split.clone(),
        tiers: args.tiers.clone(),
        methods: args.methods.clone(),
        holdout_batch: args.holdout_batch.clone(),
        labels_as_of: args.labels_as_of.unwrap_or_else(chrono::Utc::now),
        limit: args.limit,
    };
    let Some(reference) = args.same_labels_as else {
        return Ok((options, None));
    };
    let run = persist
        .get_eval_run(reference)
        .await?
        .with_context(|| format!("no eval run {reference}"))?;
    let stored = LabelOptions::from_argv(&run.argv)
        .with_context(|| format!("run {reference} has no label options"))?;
    options = stored;
    Ok((options, Some(run.label_ids_hash)))
}

async fn frozen_judge(persist: &PersistHandle, run_id: i64) -> Result<RunJudge> {
    let run = persist
        .get_eval_run(run_id)
        .await?
        .with_context(|| format!("no eval run {run_id}"))?;
    let bundle = persist
        .get_bundle(run.bundle_id)
        .await?
        .with_context(|| format!("run {run_id}'s bundle {} is gone", run.bundle_id))?;
    judge_from_components(&bundle.components).with_context(|| {
        format!(
            "bundle {} does not describe a judge this build can run",
            run.bundle_id
        )
    })
}

fn print_outcome(outcome: &RunOutcome, json: bool) -> Result<i32> {
    match outcome {
        RunOutcome::DryRun {
            items,
            strata,
            estimate_usd,
        } => {
            println!(
                "{items} item(s); strata {strata}; estimated cost ${estimate_usd:.4}; no LLM call made"
            );
            Ok(0)
        }
        RunOutcome::Refused(reason) => {
            eprintln!("refused: {reason}");
            Ok(EXIT_REFUSED)
        }
        RunOutcome::Finished {
            run_id,
            status,
            report,
            pause,
        } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"run_id": run_id, "status": status, "report": report, "pause": pause})
                    )?
                );
            } else {
                println!("eval run {run_id}: {status}");
                if let Some(report) = report {
                    println!("{}", serde_json::to_string_pretty(report)?);
                }
                if let Some(pause) = pause {
                    println!("paused: {pause}; continue with --resume {run_id}");
                }
            }
            Ok(if status == "paused_budget" {
                EXIT_PAUSED
            } else {
                0
            })
        }
    }
}

pub(super) async fn run(args: EvalRunArgs, cli: CliOverrides) -> Result<i32> {
    let config = crate::config::load::load(cli.clone())
        .context("failed to resolve config for mike eval run")?;
    let persist = super::connect(cli, true).await?;
    let overlay = crate::settings::apply_db_rows_to_overlay(persist.fetch_settings().await?);
    if !overlay.llm_record_usage {
        eprintln!(
            "warning: llm.record_usage is off, so the spend guard under-counts today's spend"
        );
    }
    let judge = match (args.resume, args.bundle) {
        (Some(run_id), _) => frozen_judge(&persist, run_id).await?,
        (None, Some(bundle_id)) => load_bundle_judge(&persist, bundle_id).await?,
        (None, None) => build_judge(&args, &overlay, &config)?,
    };
    let tracker = TaskTracker::new();
    let sink: Arc<dyn LlmUsageSink> =
        Arc::new(PersistLlmUsageSink::new(persist.clone(), tracker.clone()).await);
    install_governor(&overlay, &sink);
    let llm = OpenRouterClient::new(
        api_key(&config, &overlay, judge.model())?,
        judge.model().to_owned(),
        None,
        None,
    )?
    .with_attribution(LlmRole::Eval, Some(&sink));
    let ceiling = overlay.llm_daily_spend_ceiling_usd;
    let limits = SpendLimits {
        ceiling_usd: ceiling,
        eval_allowance_usd: eval_allowance_usd(ceiling, share_pcts(&overlay)),
        run_max_usd: args.max_usd,
    };
    let runner = Runner {
        persist: &persist,
        llm: &llm,
        limits,
    };
    let outcome = if let Some(run_id) = args.resume {
        runner.resume(run_id, &judge).await?
    } else {
        let (labels, expected_hash) = label_options(&persist, &args).await?;
        let request = StartRequest {
            bundle_id: args.bundle,
            judge: judge.clone(),
            labels,
            purpose: args.purpose.clone(),
            repeat_of: args.repeat_of,
            max_usd: args.max_usd,
            expected_hash,
            dry_run: args.dry_run,
        };
        runner.start(request).await?
    };
    tracker.close();
    tracker.wait().await;
    print_outcome(&outcome, args.json)
}
