use std::path::{Path, PathBuf};

use serde_json::json;

use super::super::args_runs::EvalGateArgs;
use super::gate;

fn write_record(dir: &Path, name: &str, time: f64, faults: f64) -> anyhow::Result<PathBuf> {
    let path = dir.join(name);
    let record = json!({
        "time_to_done_minutes": time,
        "fault_minutes": { "total": faults },
        "tokens_per_accepted_task": { "total": 1000.0 },
        "manual_interventions": { "total": 2.0 },
    });
    std::fs::write(&path, serde_json::to_vec(&record)?)?;
    Ok(path)
}

fn gate_args(before: PathBuf, after: PathBuf) -> EvalGateArgs {
    EvalGateArgs {
        candidate: None,
        baseline: None,
        kind: Some("fleet".to_owned()),
        baseline_record: Some(before),
        candidate_record: Some(after),
        history_records: Vec::new(),
        role: "turn_judge".to_owned(),
        noise: None,
        rule: "promotion".to_owned(),
        override_reason: None,
        json: true,
    }
}

#[test]
fn seeded_after_record_inside_the_band_exits_zero() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let before = write_record(dir.path(), "before.json", 100.0, 20.0)?;
    let after = write_record(dir.path(), "after.json", 105.0, 21.0)?;
    assert_eq!(gate(&gate_args(before, after))?, 0);
    Ok(())
}

#[test]
fn seeded_after_record_outside_the_band_exits_one() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let before = write_record(dir.path(), "before.json", 100.0, 20.0)?;
    let after = write_record(dir.path(), "after.json", 100.0, 40.0)?;
    assert_eq!(gate(&gate_args(before, after))?, 1);
    Ok(())
}

#[test]
fn a_missing_record_file_is_an_error_naming_the_path() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let before = write_record(dir.path(), "before.json", 100.0, 20.0)?;
    let missing = dir.path().join("absent.json");
    let Err(err) = gate(&gate_args(before, missing)) else {
        anyhow::bail!("a missing record must not gate");
    };
    assert!(err.to_string().contains("absent.json"));
    Ok(())
}
