use std::time::Duration;

use super::MIN_TMUX_MINOR;
use super::cli::{INSTALL_REMEDY, TmuxCli, argv};
use crate::launch::backend::{BackendId, Check, ProbeReport};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn probe(cli: &TmuxCli) -> ProbeReport {
    let mut checks = Vec::with_capacity(2);
    let mut report = ProbeReport {
        backend: BackendId::Tmux,
        available: false,
        version: None,
        protocol: None,
        checks: Vec::new(),
    };

    let text = match cli.run_text(&argv(&["-V"]), PROBE_TIMEOUT).await {
        Ok(text) => text,
        Err(err) => {
            checks.push(Check::fail(
                "tmux on PATH",
                err.to_string(),
                err.remedy().unwrap_or(INSTALL_REMEDY),
            ));
            report.checks = checks;
            return report;
        }
    };
    let version = text.trim().to_string();
    checks.push(Check::pass(
        "tmux on PATH",
        format!("{version} at {}", cli.bin().display()),
    ));

    match parse_version(&version) {
        Some((major, minor)) => {
            report.version = Some(format!("{major}.{minor}"));
            checks.push(version_check(major, minor));
        }
        None => checks.push(Check::fail(
            "tmux version",
            format!("could not parse a version out of `{version}`"),
            "install a standard tmux build (e.g. from your distro's package manager)",
        )),
    }

    report.available = checks.iter().all(|c| c.passed);
    report.checks = checks;
    report
}

fn parse_version(text: &str) -> Option<(u32, u32)> {
    let numeric = text.rsplit(' ').next()?.trim_start_matches("next-").trim();
    let mut parts = numeric.splitn(2, '.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor_raw = parts.next().unwrap_or("0");
    let minor_digits: String = minor_raw.chars().take_while(char::is_ascii_digit).collect();
    let minor: u32 = if minor_digits.is_empty() {
        0
    } else {
        minor_digits.parse().ok()?
    };
    Some((major, minor))
}

fn version_check(major: u32, minor: u32) -> Check {
    let (min_major, min_minor) = (3, MIN_TMUX_MINOR);
    if major > min_major || (major == min_major && minor >= min_minor) {
        Check::pass("tmux version", format!("{major}.{minor}"))
    } else {
        Check::fail(
            "tmux version",
            format!(
                "tmux {major}.{minor} is older than {min_major}.{min_minor}, needed for `-e` \
                 per-pane environment injection"
            ),
            format!("upgrade tmux to {min_major}.{min_minor} or newer"),
        )
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn parse_version_plain_and_suffixed_and_next_branch() {
        assert_eq!(parse_version("tmux 3.7c"), Some((3, 7)));
        assert_eq!(parse_version("tmux 3.3a"), Some((3, 3)));
        assert_eq!(parse_version("tmux next-3.4"), Some((3, 4)));
        assert_eq!(parse_version("tmux 2"), Some((2, 0)));
        assert_eq!(parse_version("garbage"), None);
    }

    #[test]
    fn version_check_below_minimum_fails_with_remedy() {
        let ok = version_check(3, MIN_TMUX_MINOR);
        assert!(ok.passed);
        let newer = version_check(4, 0);
        assert!(newer.passed);
        let old = version_check(3, MIN_TMUX_MINOR.saturating_sub(1));
        assert!(!old.passed);
        assert!(old.remedy.is_some());
    }
}
