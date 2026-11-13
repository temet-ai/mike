pub mod assets;
pub mod rows;
pub mod strata;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use assets::{Assets, load_assets};
use rows::{GtContext, SeedRow};
use strata::{REAL_GT_PER_STRATUM, SyntheticReason};

pub const SOURCE_KEY: &str = rows::FLEET_RUN;

pub const EXPECTED_GROUND_TRUTH: usize = 278;
pub const EXPECTED_SESSION_ENDS: usize = 544;
pub const EXPECTED_MAPPED: usize = 61;
pub const EXPECTED_CENSUS: usize = 2_111;
pub const EXPECTED_CENSUS_CORRECTED: usize = 864;
pub const EXPECTED_TAGGED: usize = 864;
pub const EXPECTED_TURNS: usize = 4_485;
pub const EXPECTED_ROWS: usize = 5_277;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExclusionPlan {
    pub session_uuid: String,
    pub turn_id: Option<i64>,
    pub reason: &'static str,
}

#[derive(Debug, Clone)]
pub struct ImportPlan {
    pub rows: Vec<SeedRow>,
    pub turns: BTreeSet<i64>,
    pub session_of_turn: HashMap<i64, String>,
    pub exclusions: Vec<ExclusionPlan>,
    pub manifest: Value,
    pub counts: Value,
}

fn check(name: &str, got: usize, want: usize) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{name}: expected {want}, found {got}"))
    }
}

pub fn check_counts(assets: &Assets) -> Result<(), String> {
    check(
        "ground_truth rows",
        assets.ground_truth.rows.len(),
        EXPECTED_GROUND_TRUTH,
    )?;
    check(
        "session_ends rows",
        assets.session_ends.rows.len(),
        EXPECTED_SESSION_ENDS,
    )?;
    check(
        "session_ends mapping.mapped",
        assets.session_ends.mapping.mapped.len(),
        EXPECTED_MAPPED,
    )?;
    check("census rows", assets.census.len(), EXPECTED_CENSUS)?;
    let corrected: BTreeSet<i64> = assets
        .census
        .iter()
        .filter(|c| c.label.eq_ignore_ascii_case("corrected"))
        .map(|c| c.turn_id)
        .collect();
    check(
        "census CORRECTED",
        corrected.len(),
        EXPECTED_CENSUS_CORRECTED,
    )?;
    check("mirror tagged", assets.mirror.tagged.len(), EXPECTED_TAGGED)?;
    let tagged: BTreeSet<i64> = assets.mirror.tagged.iter().map(|t| t.id).collect();
    if tagged != corrected {
        return Err("mirror tagged turns differ from the census CORRECTED turns".to_owned());
    }
    check("turns.jsonl lines", assets.turns.len(), EXPECTED_TURNS)
}

fn check_real_ground_truth(
    assets: &Assets,
    synthetic: &HashMap<i64, SyntheticReason>,
) -> Result<(), String> {
    let gt_ids: HashSet<i64> = assets.ground_truth.rows.iter().map(|r| r.turn_id).collect();
    let per = strata::real_gt_by_stratum(&gt_ids, &assets.turns, synthetic);
    for (era, outcome, _) in strata::EXPECTED_STRATA {
        let got = per.get(&(era, outcome.to_owned())).copied().unwrap_or(0);
        check(
            &format!("real ground-truth turns in {era}/{outcome}"),
            got,
            REAL_GT_PER_STRATUM,
        )?;
    }
    Ok(())
}

fn exclusions(assets: &Assets, synthetic: &HashMap<i64, SyntheticReason>) -> Vec<ExclusionPlan> {
    let mut sessions: BTreeMap<(String, &'static str), ()> = BTreeMap::new();
    let mut out = Vec::new();
    for turn in &assets.turns {
        match synthetic.get(&turn.turn_id) {
            Some(SyntheticReason::ProbePrompt) => out.push(ExclusionPlan {
                session_uuid: turn.session_uuid.clone(),
                turn_id: Some(turn.turn_id),
                reason: SyntheticReason::ProbePrompt.code(),
            }),
            Some(reason) => {
                sessions.insert((turn.session_uuid.clone(), reason.code()), ());
            }
            None => {}
        }
    }
    out.extend(
        sessions
            .into_keys()
            .map(|(session_uuid, reason)| ExclusionPlan {
                session_uuid,
                turn_id: None,
                reason,
            }),
    );
    out
}

fn tally(rows: &[SeedRow]) -> Value {
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        *by.entry(format!("{}/{}/{}", row.source, row.axis, row.label))
            .or_default() += 1;
    }
    json!(by)
}

pub fn plan(dir: &Path, controls: &[i64]) -> anyhow::Result<ImportPlan> {
    let assets = load_assets(dir)?;
    check_counts(&assets).map_err(anyhow::Error::msg)?;
    let synthetic = strata::synthetic(&assets.turns);
    let strata_sizes = strata::real_strata(&assets.turns, &synthetic);
    strata::check_strata(&strata_sizes).map_err(anyhow::Error::msg)?;
    check_real_ground_truth(&assets, &synthetic).map_err(anyhow::Error::msg)?;
    let strata_of = rows::strata_of(&assets);
    let real = rows::real_turns(&assets, &synthetic);
    let ctx = GtContext {
        strata: &strata_of,
        stratum_sizes: &strata_sizes,
        real: &real,
        controls,
    };
    let mut all = rows::ground_truth_rows(&assets, &ctx);
    all.extend(rows::session_end_rows(&assets, controls));
    all.extend(rows::census_rows(&assets, controls));
    all.extend(rows::mirror_rows(&assets));
    if all.len() != EXPECTED_ROWS {
        anyhow::bail!("planned {} label rows, expected {EXPECTED_ROWS}", all.len());
    }
    let turns: BTreeSet<i64> = all.iter().map(|r| r.subject_id).collect();
    let exclusions = exclusions(&assets, &synthetic);
    let manifest = json!(
        assets
            .manifest
            .iter()
            .map(|m| json!({"file": m.name, "sha256": m.sha256, "bytes": m.bytes}))
            .collect::<Vec<_>>()
    );
    let counts = json!({
        "rows": all.len(),
        "turns": turns.len(),
        "by_source_axis_label": tally(&all),
        "exclusions": exclusions.len(),
        "inclusion_prob_set": all.iter().filter(|r| r.inclusion_prob.is_some()).count(),
    });
    Ok(ImportPlan {
        rows: all,
        turns,
        session_of_turn: assets
            .turns
            .iter()
            .map(|t| (t.turn_id, t.session_uuid.clone()))
            .collect(),
        exclusions,
        manifest,
        counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../artifacts/threads/fleet-mirror")
    }

    #[test]
    fn the_plan_over_the_committed_assets_has_the_design_counts() -> anyhow::Result<()> {
        let dir = assets_dir();
        if !dir.join("ground_truth.json").exists() {
            return Ok(());
        }
        let plan = plan(&dir, &[16368])?;
        assert_eq!(plan.rows.len(), EXPECTED_ROWS);
        let by_source = |source: &str| plan.rows.iter().filter(|r| r.source == source).count();
        assert_eq!(by_source("fleet_mirror/ground_truth"), 1_390);
        assert_eq!(by_source("fleet_mirror/session_ends"), 544 + 141);
        assert_eq!(by_source("fleet_mirror/census"), 2_111);
        assert_eq!(by_source("fleet_mirror/mirror"), 1_091);
        let controls = plan
            .rows
            .iter()
            .filter(|r| r.provenance.get("control") == Some(&serde_json::json!(true)))
            .count();
        assert!(controls > 0, "turn 16368 is marked as a gate control");
        Ok(())
    }
}
