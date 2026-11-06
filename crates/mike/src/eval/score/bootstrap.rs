pub const RESAMPLES: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        usize::try_from(self.next_u64() % (n as u64)).unwrap_or(0)
    }
}

fn percentile(mut stats: Vec<f64>) -> Option<(f64, f64)> {
    if stats.is_empty() {
        return None;
    }
    stats.sort_by(f64::total_cmp);
    let len = stats.len();
    let low_idx = (0.025 * len as f64) as usize;
    let high_idx = ((0.975 * len as f64) as usize)
        .saturating_sub(1)
        .max(low_idx);
    Some((stats[low_idx.min(len - 1)], stats[high_idx.min(len - 1)]))
}

pub fn bootstrap_ci<T, F>(items: &[T], seed: u64, stat: F) -> Option<(f64, f64)>
where
    F: Fn(&[&T]) -> Option<f64>,
{
    if items.is_empty() {
        return None;
    }
    let mut rng = SplitMix64::new(seed);
    let mut stats = Vec::with_capacity(RESAMPLES);
    for _ in 0..RESAMPLES {
        let sample: Vec<&T> = (0..items.len())
            .map(|_| &items[rng.below(items.len())])
            .collect();
        if let Some(v) = stat(&sample).filter(|v| v.is_finite()) {
            stats.push(v);
        }
    }
    percentile(stats)
}

pub fn stratified_ci<T, F, S>(items: &[T], stratum: S, seed: u64, stat: F) -> Option<(f64, f64)>
where
    F: Fn(&[&T]) -> Option<f64>,
    S: Fn(&T) -> &str,
{
    let mut strata: std::collections::BTreeMap<&str, Vec<&T>> = std::collections::BTreeMap::new();
    for item in items {
        strata.entry(stratum(item)).or_default().push(item);
    }
    if strata.is_empty() {
        return None;
    }
    let mut rng = SplitMix64::new(seed);
    let mut stats = Vec::with_capacity(RESAMPLES);
    for _ in 0..RESAMPLES {
        let mut sample: Vec<&T> = Vec::with_capacity(items.len());
        for members in strata.values() {
            for _ in 0..members.len() {
                sample.push(members[rng.below(members.len())]);
            }
        }
        if let Some(v) = stat(&sample).filter(|v| v.is_finite()) {
            stats.push(v);
        }
    }
    percentile(stats)
}
