use std::collections::BTreeMap;

use super::types::{Arms, Lane, Usage, UsageRow};

const PERMUTATION_ROUNDS: usize = 4000;
pub(super) const PERMUTATION_SEED: u64 = 7;
const TIE_TOLERANCE: f64 = 1e-12;

pub(super) fn weighted(usage: &Usage) -> f64 {
    usage.fresh as f64 * 1.0
        + usage.cache_write as f64 * 1.25
        + usage.cache_read as f64 * 0.1
        + usage.output as f64 * 5.0
}

pub(super) fn roll_up(rows: &[UsageRow]) -> (BTreeMap<i64, Usage>, BTreeMap<Lane, Usage>) {
    let mut cost: BTreeMap<i64, Usage> = BTreeMap::new();
    let mut lanes: BTreeMap<Lane, Usage> = BTreeMap::new();
    for row in rows {
        cost.entry(row.who.session_id)
            .or_insert_with(Usage::empty)
            .add(row);
        if let Some(lane) = row.who.lane().filter(Lane::has_agent) {
            lanes.entry(lane).or_insert_with(Usage::empty).add(row);
        }
    }
    (cost, lanes)
}

pub(super) fn per_lane(arms: &Arms, lanes: &BTreeMap<Lane, Usage>, arm: &str) -> Vec<f64> {
    lanes
        .iter()
        .filter(|(lane, usage)| {
            arms.get(&lane.session_id).map(String::as_str) == Some(arm) && usage.complete_usage
        })
        .map(|(_, usage)| weighted(usage).max(1.0))
        .collect()
}

pub(super) fn per_turn(
    arms: &Arms,
    cost: &BTreeMap<i64, Usage>,
    turns: &BTreeMap<i64, i64>,
    arm: &str,
) -> Vec<f64> {
    arms.iter()
        .filter(|(_, assigned)| assigned.as_str() == arm)
        .filter_map(|(session, _)| {
            let usage = cost.get(session).filter(|usage| usage.complete_usage)?;
            let count = turns.get(session).copied().filter(|count| *count != 0)?;
            Some((weighted(usage) / count as f64).max(1.0))
        })
        .collect()
}

pub(super) fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    Some(if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    })
}

struct SplitMix(u64);

impl SplitMix {
    fn step(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.step() % bound as u64) as usize
    }

    fn shuffle(&mut self, items: &mut [f64]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

fn mean(values: &[f64]) -> f64 {
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for value in values {
        let total = sum + value;
        compensation += if sum.abs() >= value.abs() {
            (sum - total) + value
        } else {
            (value - total) + sum
        };
        sum = total;
    }
    (sum + compensation) / values.len() as f64
}

pub(super) fn permutation_p(a: &[f64], b: &[f64]) -> f64 {
    permutation_p_seeded(a, b, PERMUTATION_ROUNDS, PERMUTATION_SEED)
}

pub(super) fn permutation_p_seeded(a: &[f64], b: &[f64], rounds: usize, seed: u64) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 1.0;
    }
    let la: Vec<f64> = a.iter().map(|x| x.ln()).collect();
    let lb: Vec<f64> = b.iter().map(|x| x.ln()).collect();
    let observed = (mean(&la) - mean(&lb)).abs();
    let mut pool: Vec<f64> = la.iter().chain(lb.iter()).copied().collect();
    let split = la.len();
    let mut rng = SplitMix(seed);
    let mut hits = 0usize;
    for _ in 0..rounds {
        rng.shuffle(&mut pool);
        let (left, right) = pool.split_at(split);
        if (mean(left) - mean(right)).abs() >= observed - TIE_TOLERANCE {
            hits += 1;
        }
    }
    (hits + 1) as f64 / (rounds + 1) as f64
}
