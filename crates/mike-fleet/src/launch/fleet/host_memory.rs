const KIB_PER_GIB: f64 = 1024.0 * 1024.0;
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn parse_meminfo(text: &str) -> Option<f64> {
    let line = text
        .lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))?;
    let kib: f64 = line.trim().trim_end_matches("kB").trim().parse().ok()?;
    Some(kib / KIB_PER_GIB)
}

pub fn parse_vm_stat(text: &str) -> Option<f64> {
    let header = text.lines().next()?;
    let page_bytes: f64 = header
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |name: &str| -> Option<f64> {
        text.lines()
            .find_map(|line| line.strip_prefix(name))?
            .trim()
            .trim_end_matches('.')
            .parse()
            .ok()
    };
    let available =
        pages("Pages free:")? + pages("Pages inactive:")? + pages("Pages speculative:")?;
    Some(available * page_bytes / BYTES_PER_GIB)
}

pub async fn free_gb() -> Option<f64> {
    if cfg!(target_os = "macos") {
        let out = tokio::process::Command::new("vm_stat")
            .output()
            .await
            .ok()?;
        return parse_vm_stat(&String::from_utf8_lossy(&out.stdout));
    }
    parse_meminfo(&tokio::fs::read_to_string("/proc/meminfo").await.ok()?)
}

pub fn allows(floor_gb: Option<f64>, free_gb: Option<f64>) -> bool {
    match (floor_gb, free_gb) {
        (Some(floor), Some(free)) => free > floor,
        _ => true,
    }
}

#[cfg(test)]
#[path = "host_memory_tests.rs"]
mod tests;
