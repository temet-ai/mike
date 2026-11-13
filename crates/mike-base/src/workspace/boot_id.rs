use std::sync::LazyLock;

pub static CURRENT_BOOT_ID: LazyLock<Option<String>> = LazyLock::new(compute_boot_id);

#[cfg(target_os = "macos")]
fn compute_boot_id() -> Option<String> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "kern.boottime"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let sec = text
        .split("sec = ")
        .nth(1)?
        .split(',')
        .next()?
        .trim()
        .parse::<i64>()
        .ok()?;
    Some(sec.to_string())
}

#[cfg(target_os = "linux")]
fn compute_boot_id() -> Option<String> {
    let text = std::fs::read_to_string("/proc/stat").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("btime ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn compute_boot_id() -> Option<String> {
    None
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn current_boot_id_resolves_on_supported_platforms() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert!(
                CURRENT_BOOT_ID.as_deref().is_some_and(|s| !s.is_empty()),
                "expected a resolvable boot id on this platform"
            );
        }
    }
}
