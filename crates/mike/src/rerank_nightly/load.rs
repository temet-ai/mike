pub(super) fn load_per_cpu() -> Option<f64> {
    let mut averages = [0.0f64; 3];
    let read = unsafe { libc::getloadavg(averages.as_mut_ptr(), 1) };
    if read < 1 {
        return None;
    }
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    Some(averages[0] / cpus as f64)
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
