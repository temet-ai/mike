use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::snapshot::CrossSessionSnapshot;
use super::threshold_pass::{RowSpec, build_row};
use super::weighting::{BinInput, Tally, bin_edge, weigh};
use crate::store::persistence::cross_session::{
    FireBinRow, FireSteerer, RollupRef, ThresholdRow, ThresholdWrite,
};
use crate::store::persistence::{PersistError, PersistHandle};

const FIRE_FLOOR: f64 = 0.6;

fn bin_inputs(steerer: &FireSteerer, bins: &[FireBinRow]) -> Vec<BinInput> {
    let mut by_edge: BTreeMap<&str, BinInput> = BTreeMap::new();
    for row in bins.iter().filter(|b| b.steerer_id == steerer.id) {
        let Some(edge) = bin_edge(Some(&row.lower_edge)) else {
            continue;
        };
        by_edge
            .entry(row.lower_edge.as_str())
            .or_insert_with(|| BinInput {
                lower_edge: edge,
                sources: BTreeMap::new(),
            })
            .sources
            .insert(
                row.label_source.clone(),
                Tally {
                    k: f64::from(row.k),
                    n: f64::from(row.n),
                },
            );
    }
    by_edge.into_values().collect()
}

fn previous_for(in_force: &[ThresholdRow], steerer_id: i64) -> Option<(f32, DateTime<Utc>)> {
    in_force
        .iter()
        .filter(|t| t.steerer_id == steerer_id && t.decision == "applied")
        .max_by_key(|t| (t.computed_at, t.id))
        .map(|t| (t.applied_threshold, t.computed_at))
}

pub fn plan_fire(
    snapshot: &CrossSessionSnapshot,
    steerers: &[FireSteerer],
    bins: &[FireBinRow],
    rollup: RollupRef,
    now: DateTime<Utc>,
) -> Vec<ThresholdWrite> {
    steerers
        .iter()
        .map(|steerer| {
            let weighted = weigh(&bin_inputs(steerer, bins), &snapshot.settings);
            build_row(
                &snapshot.settings,
                &RowSpec {
                    steerer_id: steerer.id,
                    question: "fire",
                    project: None,
                    rollup,
                    question_mode: "append",
                    bar: snapshot.settings.steerer_bar,
                    floor: FIRE_FLOOR,
                    static_threshold: steerer.static_threshold,
                    previous: previous_for(&snapshot.fire_thresholds, steerer.id),
                    steerer_name: Some(&steerer.name),
                    now,
                },
                &weighted,
            )
        })
        .collect()
}

pub async fn plan_fire_rows(
    persist: &PersistHandle,
    snapshot: &CrossSessionSnapshot,
    rollup: RollupRef,
    now: DateTime<Utc>,
) -> Result<Vec<ThresholdWrite>, PersistError> {
    let steerers = persist.steerer().fire_steerers().await?;
    if steerers.is_empty() {
        return Ok(Vec::new());
    }
    let bins = persist.steerer().fetch_fire_bins(now).await?;
    Ok(plan_fire(snapshot, &steerers, &bins, rollup, now))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn bin(steerer_id: i64, edge: &str, source: &str, k: i32, n: i32) -> FireBinRow {
        FireBinRow {
            steerer_id,
            steerer_name: "verifier".to_owned(),
            lower_edge: edge.to_owned(),
            label_source: source.to_owned(),
            k,
            n,
        }
    }

    fn steerer(id: i64, name: &str) -> FireSteerer {
        FireSteerer {
            id,
            name: name.to_owned(),
            static_threshold: 0.7,
        }
    }

    #[test]
    fn bin_inputs_merges_label_sources_into_one_bin_per_edge() {
        let inputs = bin_inputs(
            &steerer(6, "verifier"),
            &[
                bin(6, "0.95", "session_reply", 29, 70),
                bin(6, "0.95", "operator", 1, 2),
                bin(6, "0.90", "session_reply", 16, 28),
                bin(15, "0.95", "session_reply", 88, 142),
            ],
        );
        assert_eq!(inputs.len(), 2);
        let top = inputs.iter().find(|i| (i.lower_edge - 0.95).abs() < 1e-9);
        assert_eq!(top.map(|i| i.sources.len()), Some(2));
    }

    #[test]
    fn plan_fire_writes_one_row_per_steerer_even_without_labels() {
        let snapshot = CrossSessionSnapshot::default();
        let rollup = RollupRef {
            id: 1,
            finished_at: Utc::now(),
        };
        let rows = plan_fire(
            &snapshot,
            &[steerer(6, "verifier"), steerer(15, "monitor-over-sleep")],
            &[bin(6, "0.95", "session_reply", 29, 70)],
            rollup,
            Utc::now(),
        );
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|r| r.question == "fire" && r.scope == "global")
        );
        assert_eq!(rows[0].n_labelled, 70);
        assert_eq!(rows[1].n_labelled, 0);
        assert_eq!(rows[1].decision, "mode_off");
    }

    #[test]
    fn previous_for_picks_the_latest_applied_row_of_that_steerer() {
        let mk = |id: i64, steerer_id: i64, at_secs: i64, t: f32| ThresholdRow {
            id,
            steerer_id,
            question: "fire".to_owned(),
            scope: "global".to_owned(),
            project_id: None,
            computed_at: DateTime::from_timestamp(at_secs, 0).unwrap_or_default(),
            rollup_id: 1,
            applied_threshold: t,
            decision: "applied".to_owned(),
            detail: serde_json::json!({}),
        };
        let rows = [mk(1, 6, 100, 0.8), mk(2, 6, 200, 0.85), mk(3, 15, 300, 0.9)];
        assert_eq!(previous_for(&rows, 6).map(|p| p.0), Some(0.85));
        assert_eq!(previous_for(&rows, 99), None);
    }
}
