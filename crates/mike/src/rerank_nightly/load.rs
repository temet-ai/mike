const LOADAVG: &str = "/proc/loadavg";

pub(super) fn load_per_cpu() -> Option<f64> {
    let text = std::fs::read_to_string(LOADAVG).ok()?;
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    one_minute(&text).map(|load| load / cpus as f64)
}

pub(super) fn one_minute(loadavg: &str) -> Option<f64> {
    loadavg.split_whitespace().next()?.parse().ok()
}

pub(super) fn under_ceiling(load: Option<f64>, ceiling: f64) -> bool {
    match load {
        Some(load) => load <= ceiling,
        None => {
            tracing::warn!("rerank_nightly: load average unreadable; treating the machine as busy");
            false
        }
    }
}
