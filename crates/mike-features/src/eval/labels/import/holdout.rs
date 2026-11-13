use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const AXES_PER_TURN: usize = 5;
pub const HOLDOUT_TIER: &str = "seed";
pub const HOLDOUT_METHOD: &str = "double_blind_adjudicated";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldoutRow {
    pub subject_kind: String,
    pub subject_id: i64,
    pub operator_turn_id: i64,
    pub axis: String,
    pub label: String,
    pub tier: String,
    pub method: String,
    pub split: String,
    pub source: String,
    pub confidence: Option<String>,
    pub evidence: Option<String>,
    pub inclusion_prob: Option<f64>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub severity: Option<String>,
    pub provenance: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HoldoutPlan {
    pub batch: String,
    pub rows: Vec<HoldoutRow>,
    pub turns: BTreeSet<i64>,
    pub manifest: Value,
    pub counts: Value,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    std::fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))
}

fn manifest_turns(manifest: &Value) -> Result<BTreeSet<i64>, String> {
    if let Some(ids) = manifest.get("turn_ids").and_then(Value::as_array) {
        return Ok(ids.iter().filter_map(Value::as_i64).collect());
    }
    let turns = manifest
        .get("turns")
        .and_then(Value::as_array)
        .ok_or("manifest.json has neither turn_ids nor turns")?;
    Ok(turns
        .iter()
        .filter_map(|t| {
            t.get("turn_id")
                .and_then(Value::as_i64)
                .or_else(|| t.as_i64())
        })
        .collect())
}

fn parse_rows(text: &str) -> Result<Vec<HoldoutRow>, String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).map_err(|e| format!("import.jsonl line {}: {e}", i + 1))
        })
        .collect()
}

fn check_rows(
    rows: &[HoldoutRow],
    turns: &BTreeSet<i64>,
    batch: &str,
    rubric_sha: &str,
) -> Result<(), String> {
    let source = format!("jc11/{batch}");
    let mut per_turn: BTreeMap<i64, BTreeSet<&str>> = BTreeMap::new();
    for row in rows {
        if row.split != "holdout" || row.tier != HOLDOUT_TIER || row.method != HOLDOUT_METHOD {
            return Err(format!(
                "turn {} axis {}: not a seed/double_blind_adjudicated holdout row",
                row.operator_turn_id, row.axis
            ));
        }
        if row.source != source {
            return Err(format!(
                "turn {}: source {:?}, expected {source:?}",
                row.operator_turn_id, row.source
            ));
        }
        if row.provenance.get("rubric_sha256").and_then(Value::as_str) != Some(rubric_sha) {
            return Err(format!(
                "turn {}: labelled against a different rubric",
                row.operator_turn_id
            ));
        }
        per_turn
            .entry(row.operator_turn_id)
            .or_default()
            .insert(row.axis.as_str());
    }
    let labelled: BTreeSet<i64> = per_turn.keys().copied().collect();
    if &labelled != turns {
        return Err(format!(
            "import.jsonl covers {} turn(s) but the manifest lists {}",
            labelled.len(),
            turns.len()
        ));
    }
    if let Some((turn, axes)) = per_turn
        .iter()
        .find(|(_, axes)| axes.len() != AXES_PER_TURN)
    {
        return Err(format!(
            "turn {turn} has {} axes, expected {AXES_PER_TURN}",
            axes.len()
        ));
    }
    Ok(())
}

fn counts(rows: &[HoldoutRow]) -> Value {
    let mut by_axis: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut by_stratum: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        *by_axis
            .entry(row.axis.clone())
            .or_default()
            .entry(row.label.clone())
            .or_default() += 1;
        if row.axis == "reaction" {
            let stratum = row
                .provenance
                .get("stratum")
                .and_then(Value::as_str)
                .unwrap_or("all");
            *by_stratum.entry(stratum.to_owned()).or_default() += 1;
        }
    }
    json!({ "rows": rows.len(), "by_axis": by_axis, "by_stratum": by_stratum })
}

pub fn plan(dir: &Path, batch: &str) -> Result<HoldoutPlan, String> {
    let manifest: Value = serde_json::from_slice(&read(dir, "manifest.json")?)
        .map_err(|e| format!("manifest.json: {e}"))?;
    let blind_sha = sha256_hex(&read(dir, "sample-blind.json")?);
    if manifest.get("blind_sha256").and_then(Value::as_str) != Some(blind_sha.as_str()) {
        return Err(
            "sample-blind.json changed after the manifest was written (blind_sha256 differs)"
                .to_owned(),
        );
    }
    let rubric_sha = manifest
        .get("rubric_sha256")
        .and_then(Value::as_str)
        .ok_or("manifest.json has no rubric_sha256")?
        .to_owned();
    let text =
        String::from_utf8(read(dir, "import.jsonl")?).map_err(|e| format!("import.jsonl: {e}"))?;
    let rows = parse_rows(&text)?;
    let turns = manifest_turns(&manifest)?;
    check_rows(&rows, &turns, batch, &rubric_sha)?;
    Ok(HoldoutPlan {
        batch: batch.to_owned(),
        counts: counts(&rows),
        rows,
        turns,
        manifest,
    })
}

#[cfg(test)]
pub mod tests {
    use super::{plan, sha256_hex};
    use serde_json::json;

    fn write_dir(rows_per_turn: usize, tamper_blind: bool) -> anyhow::Result<tempfile::TempDir> {
        let dir = tempfile::tempdir()?;
        let blind = b"[{\"turn_id\": 1}]";
        std::fs::write(dir.path().join("sample-blind.json"), blind)?;
        let manifest =
            json!({"turn_ids": [1], "blind_sha256": sha256_hex(blind), "rubric_sha256": "r"});
        std::fs::write(dir.path().join("manifest.json"), manifest.to_string())?;
        let axes = ["reaction", "delivery", "fact_a", "fact_b", "fact_c"];
        let lines: Vec<String> = axes
            .iter()
            .take(rows_per_turn)
            .map(|axis| {
                json!({"subject_kind": "turn", "subject_id": 1, "operator_turn_id": 1, "axis": axis, "label": "x",
                       "tier": "seed", "method": "double_blind_adjudicated", "split": "holdout", "source": "jc11/b",
                       "confidence": "high", "evidence": "e", "inclusion_prob": 1.0, "provenance": {"rubric_sha256": "r"}})
                .to_string()
            })
            .collect();
        std::fs::write(dir.path().join("import.jsonl"), lines.join("\n"))?;
        if tamper_blind {
            std::fs::write(dir.path().join("sample-blind.json"), b"[]")?;
        }
        Ok(dir)
    }

    #[test]
    fn a_complete_batch_plans_five_rows_per_turn() -> anyhow::Result<()> {
        let dir = write_dir(5, false)?;
        let p = plan(dir.path(), "b").map_err(anyhow::Error::msg)?;
        assert_eq!(p.rows.len(), 5);
        assert_eq!(p.turns.len(), 1);
        Ok(())
    }

    #[test]
    fn import_holdout_refuses_a_changed_blind_file() -> anyhow::Result<()> {
        let dir = write_dir(5, true)?;
        let err = plan(dir.path(), "b").err().unwrap_or_default();
        assert!(err.contains("blind_sha256"), "{err}");
        Ok(())
    }

    #[test]
    fn a_turn_missing_an_axis_is_refused() -> anyhow::Result<()> {
        let dir = write_dir(4, false)?;
        assert!(plan(dir.path(), "b").is_err());
        Ok(())
    }
}
