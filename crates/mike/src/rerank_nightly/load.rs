use std::process::Command;

const LOADAVG: &str = "/proc/loadavg";

pub(super) fn load_per_cpu() -> Option<f64> {
    let load = match std::fs::read_to_string(LOADAVG) {
        Ok(text) => one_minute(&text),
        Err(_) => sysctl_one_minute(),
    }?;
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    Some(load / cpus as f64)
}

pub(super) fn one_minute(loadavg: &str) -> Option<f64> {
    loadavg.split_whitespace().next()?.parse().ok()
}

pub(super) fn one_minute_from_sysctl(output: &str) -> Option<f64> {
    one_minute(output.trim().trim_start_matches('{'))
}

fn sysctl_one_minute() -> Option<f64> {
    let output = Command::new("sysctl")
        .args(["-n", "vm.loadavg"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    one_minute_from_sysctl(std::str::from_utf8(&output.stdout).ok()?)
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
