use crate::store::persistence::eval_repo::EvalPersist as _;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tokio_util::task::TaskTracker;

use super::args_runs::{EvalGateArgs, EvalRunArgs};
use super::run::{api_key, install_governor};
use crate::config::CliOverrides;
use crate::config::persist_bootstrap::PersistLlmUsageSink;
use crate::eval::session_end::gate::{
    SessionEndFloors, SessionEndGateRequest, decide_session_end, exit_code,
};
use crate::eval::session_end::run::{
    SessionEndRunOutcome, SessionEndRunRequest, end_items_from_rows, run_session_end_eval,
};
use crate::eval::session_end::score::score_session_end_run;
use crate::judge_worker::session_end::config::SessionJudgeV3Config;
use crate::llm::{LlmRole, LlmUsageSink, OpenRouterClient};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_runs::NewEvalGate;

const SCORE_SEED: u64 = 13;

async fn bundle_config(persist: &PersistHandle, bundle_id: i64) -> Result<SessionJudgeV3Config> {
    let bundle = persist
        .eval()
        .get_bundle(bundle_id)
        .await?
        .with_context(|| format!("no bundle {bundle_id}"))?;
    SessionJudgeV3Config::from_bundle_components(&bundle.components)
        .map_err(|e| anyhow::anyhow!("bundle {bundle_id} is not a session_judge v3 bundle: {e}"))
}

fn print_run(outcome: &SessionEndRunOutcome, json: bool) -> Result<i32> {
    match outcome {
        SessionEndRunOutcome::DryRun {
            items,
            labels,
            estimate_usd,
        } => {
            println!(
                "{items} ending turn(s); labels {labels:?}; estimated cost ${estimate_usd:.4}; no LLM call made"
            );
            Ok(0)
        }
        SessionEndRunOutcome::Finished {
            run_id,
            status,
            report,
        } => {
            let body = serde_json::json!({"run_id": run_id, "status": status, "report": report});
            if json {
                println!("{}", serde_json::to_string_pretty(&body)?);
            } else {
                println!("eval run {run_id} (session_judge): {status}");
                println!("{}", serde_json::to_string_pretty(report)?);
            }
            Ok(if status == "paused_budget" {
                super::run::EXIT_PAUSED
            } else {
                0
            })
        }
    }
}

pub(super) async fn run(args: EvalRunArgs, cli: CliOverrides) -> Result<i32> {
    let Some(bundle_id) = args.bundle else {
        bail!("mike eval run --role session_judge needs --bundle <id>");
    };
    if args.resume.is_some() || args.same_labels_as.is_some() {
        bail!(
            "--resume and --same-labels-as are turn_judge options; start a new session_judge run"
        );
    }
    let config = crate::config::load::load(cli.clone())
        .context("failed to resolve config for mike eval run")?;
    let persist = super::connect(cli, true).await?;
    let overlay = crate::settings::apply_db_rows_to_overlay(persist.fetch_settings().await?);
    let cfg = bundle_config(&persist, bundle_id).await?;
    let tracker = TaskTracker::new();
    let sink: Arc<dyn LlmUsageSink> =
        Arc::new(PersistLlmUsageSink::new(persist.clone(), tracker.clone()).await);
    install_governor(&overlay, &sink);
    let llm = OpenRouterClient::new(
        api_key(&config, &overlay, &cfg.model)?,
        cfg.model.clone(),
        None,
        None,
    )?
    .with_attribution(LlmRole::Eval, Some(&sink));
    let request = SessionEndRunRequest {
        bundle_id,
        cfg,
        split: args.split.clone(),
        tiers: args.tiers.clone(),
        labels_as_of: args.labels_as_of.unwrap_or_else(chrono::Utc::now),
        purpose: args.purpose.clone(),
        repeat_of: args.repeat_of,
        max_usd: args.max_usd,
        limit: args.limit,
        dry_run: args.dry_run,
    };
    let outcome = run_session_end_eval(&persist, &llm, &request).await?;
    tracker.close();
    tracker.wait().await;
    print_run(&outcome, args.json)
}

pub(super) async fn gate(args: EvalGateArgs, cli: CliOverrides) -> Result<i32> {
    let persist = super::connect(cli, true).await?;
    let candidate_id = args.candidate_run()?;
    let candidate_run = persist
        .eval()
        .get_eval_run(candidate_id)
        .await?
        .with_context(|| format!("no eval run {candidate_id}"))?;
    let cfg = bundle_config(&persist, candidate_run.bundle_id).await?;
    let candidate = end_items_from_rows(&persist.eval().all_session_end_items(candidate_id).await?);
    let noise = match args.noise {
        Some(id) => Some(end_items_from_rows(
            &persist.eval().all_session_end_items(id).await?,
        )),
        None => None,
    };
    let rules: Vec<&str> = cfg.default_rules.iter().map(|r| r.code()).collect();
    let decision = decide_session_end(&SessionEndGateRequest {
        candidate: &candidate,
        noise: noise.as_deref(),
        default_rules: &rules,
        floors: SessionEndFloors::default(),
    });
    let report = serde_json::json!({
        "variant": decision.variant,
        "failures": decision.failures,
        "candidate": score_session_end_run(&candidate, SCORE_SEED),
        "noise": noise.as_deref().map(|n| score_session_end_run(n, SCORE_SEED)),
        "split": candidate_run.split,
    });
    let gate_id = persist
        .eval()
        .insert_eval_gate(&NewEvalGate {
            candidate_run_id: candidate_id,
            baseline_run_id: None,
            noise_run_id: args.noise,
            holdout_batch: candidate_run.holdout_batch.as_deref(),
            verdict: decision.verdict,
            floors: decision.floors.clone(),
            report: report.clone(),
            override_reason: args.override_reason.as_deref(),
        })
        .await?;
    if decision.verdict == "pass" && candidate_run.split == "holdout" {
        super::gate::record_gate_on_bundle(&persist, &candidate_run, gate_id).await?;
    }
    let body =
        serde_json::json!({"gate_id": gate_id, "verdict": decision.verdict, "report": report});
    if args.json {
        println!("{}", serde_json::to_string_pretty(&body)?);
    } else {
        println!("gate {gate_id} (session_judge): {}", decision.verdict);
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(exit_code(decision.verdict))
}
