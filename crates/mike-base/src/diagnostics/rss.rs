#[cfg(unix)]
pub fn read_max_rss() -> Option<u64> {
    use nix::sys::resource::{UsageWho, getrusage};
    let usage = getrusage(UsageWho::RUSAGE_SELF).ok()?;
    let raw = u64::try_from(usage.max_rss()).unwrap_or(0);
    #[cfg(target_os = "macos")]
    {
        Some(raw)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(raw.saturating_mul(1024))
    }
}

#[cfg(not(unix))]
pub fn read_max_rss() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::read_max_rss;

    #[cfg(unix)]
    #[test]
    fn read_max_rss_returns_non_zero_on_unix() -> anyhow::Result<()> {
        let rss = read_max_rss()
            .ok_or_else(|| anyhow::anyhow!("getrusage(RUSAGE_SELF) returned None"))?;
        assert!(rss > 0, "peak RSS should be non-zero, got {rss}");
        Ok(())
    }
}
