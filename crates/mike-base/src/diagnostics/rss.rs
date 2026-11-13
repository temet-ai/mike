#[cfg(unix)]
pub fn read_max_rss() -> Option<u64> {
    let rusage = unsafe {
        let mut r = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, r.as_mut_ptr()) != 0 {
            return None;
        }
        r.assume_init()
    };

    let raw = rusage.ru_maxrss.max(0) as u64;
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
pub mod tests {
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
