use anyhow::{Context, Result};

use super::args_runs::EvalGateArgs;
use crate::change_log::ChangeSource;
use crate::config::CliOverrides;
use crate::eval::gate::{Floors, GateDecision, GateItem, GateRequest, GateRule, GateRun, decide};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_bundles::StatusChange;
use crate::store::persistence::eval_runs::{EvalRunItemRow, EvalRunRow, NewEvalGate};

fn gate_item(row: &EvalRunItemRow) -> GateItem {
    GateItem {
        turn_id: row.operator_turn_id,
        outcome: row.outcome.clone(),
        gold_reaction: row.gold_reaction.clone().unwrap_or_default(),
        gold_delivery: row.gold_delivery.clone(),
        pred_reaction: row
            .verdict_reaction
            .clone()
            .or_else(|| row.verdict_legacy_label.clone()),
        pred_delivery: row.verdict_delivery.clone(),
        inclusion_prob: row.inclusion_prob,
        control: row.gold_control.unwrap_or(false),
        settled_without_reply: row.next_state.as_deref() == Some("none_session_settled"),
    }
}

async fn load_run(persist: &PersistHandle, id: i64) -> Result<(EvalRunRow, GateRun)> {
    let run = persist
        .get_eval_run(id)
        .await?
        .with_context(|| format!("no eval run {id}"))?;
    let items = persist.all_eval_run_items(id).await?;
    let gate_run = GateRun {
        run_id: run.id,
        status: run.status.clone(),
        label_ids_hash: run.label_ids_hash.clone(),
        split: run.split.clone(),
        holdout_batch: run.holdout_batch.clone(),
        repeat_of_run_id: run.repeat_of_run_id,
        r3: run.rubric_version.starts_with("r3"),
        slices: run.slices.clone(),
        items: items.iter().map(gate_item).collect(),
    };
    Ok((run, gate_run))
}

pub(super) async fn record_gate_on_bundle(
    persist: &PersistHandle,
    candidate: &EvalRunRow,
    gate_id: i64,
) -> Result<()> {
    let bundle = persist
        .get_bundle(candidate.bundle_id)
        .await?
        .with_context(|| format!("bundle {} is gone", candidate.bundle_id))?;
    let source = ChangeSource::internal()
        .with_detail("eval_gate", serde_json::json!(gate_id))
        .to_setting_json(&crate::change_log::current_build());
    persist
        .set_bundle_status(&StatusChange {
            id: bundle.id,
            status: &bundle.status,
            gate_run_id: Some(gate_id),
            override_reason: None,
            source_json: &source,
        })
        .await?;
    Ok(())
}

fn print_decision(decision: &GateDecision, gate_id: Option<i64>, json: bool) -> Result<()> {
    let body = serde_json::json!({
        "gate_id": gate_id,
        "verdict": decision.verdict,
        "floors": decision.floors,
        "report": decision.report,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&body)?);
    } else {
        println!(
            "gate {}: {}",
            gate_id.map_or("not recorded".to_owned(), |id| id.to_string()),
            decision.verdict
        );
        println!("{}", serde_json::to_string_pretty(&decision.report)?);
    }
    Ok(())
}

pub(super) async fn gate(args: EvalGateArgs, cli: CliOverrides) -> Result<i32> {
    let persist = super::connect(cli, true).await?;
    let candidate_id = args.candidate_run()?;
    let (candidate_row, candidate) = load_run(&persist, candidate_id).await?;
    let baseline_id = args
        .baseline
        .context("mike eval gate --role turn_judge needs --baseline <run id>")?;
    let (_, baseline) = load_run(&persist, baseline_id).await?;
    let noise = match args.noise {
        Some(id) => Some(load_run(&persist, id).await?.1),
        None => None,
    };
    let prior = match candidate.holdout_batch.as_deref() {
        Some(batch) => persist.count_holdout_gates(batch).await?,
        None => 0,
    };
    let rule = if args.rule == "e-slice" {
        GateRule::ESlice
    } else {
        GateRule::Promotion
    };
    let decision = decide(&GateRequest {
        rule,
        candidate: &candidate,
        baseline: &baseline,
        noise: noise.as_ref(),
        prior_holdout_gates: prior,
        override_reason: args.override_reason.as_deref(),
        floors: Floors::default(),
    });
    let gate_id = if decision.record {
        let gate = NewEvalGate {
            candidate_run_id: candidate_id,
            baseline_run_id: Some(baseline_id),
            noise_run_id: args.noise,
            holdout_batch: candidate.holdout_batch.as_deref(),
            verdict: decision.verdict,
            floors: decision.floors.clone(),
            report: decision.report.clone(),
            override_reason: args.override_reason.as_deref(),
        };
        Some(persist.insert_eval_gate(&gate).await?)
    } else {
        None
    };
    if let Some(id) = gate_id
        && decision.verdict == "pass"
        && rule == GateRule::Promotion
        && candidate.split == "holdout"
    {
        record_gate_on_bundle(&persist, &candidate_row, id).await?;
    }
    print_decision(&decision, gate_id, args.json)?;
    Ok(decision.exit_code)
}
