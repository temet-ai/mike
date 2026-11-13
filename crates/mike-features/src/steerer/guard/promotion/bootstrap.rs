use crate::eval::score::bootstrap::SplitMix64;

pub const RESAMPLES: usize = 2_000;

pub fn seed_for(question: &str, as_of_secs: i64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in question
        .as_bytes()
        .iter()
        .chain(as_of_secs.to_le_bytes().iter())
    {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn rate(sample: &[bool]) -> Option<f64> {
    (!sample.is_empty()).then(|| sample.iter().filter(|b| **b).count() as f64 / sample.len() as f64)
}

fn resample(rng: &mut SplitMix64, items: &[bool]) -> Vec<bool> {
    (0..items.len())
        .map(|_| items[rng.below(items.len())])
        .collect()
}

fn percentile(mut stats: Vec<f64>) -> Option<(f64, f64)> {
    if stats.is_empty() {
        return None;
    }
    stats.sort_by(f64::total_cmp);
    let len = stats.len();
    let low = ((0.025 * len as f64) as usize).min(len - 1);
    let high = ((0.975 * len as f64) as usize)
        .saturating_sub(1)
        .max(low)
        .min(len - 1);
    Some((stats[low], stats[high]))
}

pub fn risk_difference_ci(
    holdback_bad: &[bool],
    treated_bad: &[bool],
    seed: u64,
) -> Option<(f64, f64)> {
    if holdback_bad.is_empty() || treated_bad.is_empty() {
        return None;
    }
    let mut rng = SplitMix64::new(seed);
    let stats: Vec<f64> = (0..RESAMPLES)
        .filter_map(|_| {
            let h = rate(&resample(&mut rng, holdback_bad))?;
            let t = rate(&resample(&mut rng, treated_bad))?;
            Some(h - t)
        })
        .collect();
    percentile(stats)
}

pub fn clustered_rd_ci(
    holdback: &[Vec<bool>],
    treated: &[Vec<bool>],
    seed: u64,
) -> Option<(f64, f64)> {
    if holdback.is_empty() || treated.is_empty() {
        return None;
    }
    let mut rng = SplitMix64::new(seed);
    let flat = |rng: &mut SplitMix64, clusters: &[Vec<bool>]| -> Vec<bool> {
        (0..clusters.len())
            .flat_map(|_| clusters[rng.below(clusters.len())].clone())
            .collect()
    };
    let stats: Vec<f64> = (0..RESAMPLES)
        .filter_map(|_| {
            let h = rate(&flat(&mut rng, holdback))?;
            let t = rate(&flat(&mut rng, treated))?;
            Some(h - t)
        })
        .collect();
    percentile(stats)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn risk_difference_ci_brackets_a_clear_effect() {
        let holdback: Vec<bool> = (0..60).map(|i| i % 10 < 6).collect();
        let treated: Vec<bool> = (0..60).map(|i| i % 20 < 7).collect();
        let (low, high) = risk_difference_ci(&holdback, &treated, 11).unwrap_or((0.0, 0.0));
        assert!(low > 0.0 && high < 0.6, "({low}, {high})");
        assert_eq!(risk_difference_ci(&[], &treated, 11), None);
        assert_eq!(seed_for("relapse", 5), seed_for("relapse", 5));
    }
}
