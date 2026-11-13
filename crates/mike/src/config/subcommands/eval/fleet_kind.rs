use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::args_runs::{EvalGateArgs, EvalRunArgs};
use crate::config::CliOverrides;
use crate::eval::fleet::gate::{NoiseBand, decide};
use crate::eval::fleet::numbers::FleetNumbers;

fn read_record(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read record file {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("record file {} is not valid JSON", path.display()))
}

fn numbers_from_file(path: &Path) -> Result<FleetNumbers> {
    Ok(FleetNumbers::from_record(&read_record(path)?))
}

pub(super) fn gate(args: &EvalGateArgs) -> Result<i32> {
    let (Some(before_path), Some(after_path)) = (&args.baseline_record, &args.candidate_record)
    else {
        bail!(
            "mike eval gate --kind fleet needs --baseline-record <file> and --candidate-record <file>"
        );
    };
    let before = numbers_from_file(before_path)?;
    let after = numbers_from_file(after_path)?;
    let history = args
        .history_records
        .iter()
        .map(|path| numbers_from_file(path))
        .collect::<Result<Vec<_>>>()?;
    let decision = decide(&before, &after, &NoiseBand::from_history(&history));
    let body = json!({
        "kind": "fleet",
        "verdict": decision.verdict,
        "report": decision.report,
    });
    if args.json {
        println!("{}", serde_json::to_string_pretty(&body)?);
    } else {
        println!("fleet gate: {}", decision.verdict);
        println!("{}", serde_json::to_string_pretty(&decision.report)?);
    }
    Ok(decision.exit_code)
}

async fn record_from_database(fleet: uuid::Uuid, cli: CliOverrides) -> Result<Value> {
    let persist = super::connect(cli, false).await?;
    persist
        .fleet()
        .find_run_record(fleet)
        .await?
        .with_context(|| format!("fleet {fleet} has no run record"))
}

pub(super) async fn run(args: EvalRunArgs, cli: CliOverrides) -> Result<i32> {
    let record = match (&args.record_file, args.fleet_run) {
        (Some(path), None) => read_record(path)?,
        (None, Some(fleet)) => record_from_database(fleet, cli).await?,
        _ => bail!(
            "mike eval run --kind fleet needs one of --record-file <file> or --fleet-run <fleet uuid>"
        ),
    };
    let numbers = FleetNumbers::from_record(&record);
    let missing = numbers.missing();
    let body = json!({ "kind": "fleet", "numbers": numbers, "missing": missing });
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(if missing.is_empty() { 0 } else { 2 })
}

#[cfg(test)]
#[path = "fleet_kind_tests.rs"]
mod fleet_kind_tests;
