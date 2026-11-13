use std::time::Duration;

use serde_json::Value;

use super::cli::{HYPR_INSTALL_REMEDY, TerminalCli, XDG_INSTALL_REMEDY, argv};
use crate::launch::backend::{BackendId, Check, ProbeReport};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn probe(cli: &TerminalCli) -> ProbeReport {
    let mut checks = Vec::with_capacity(3);
    let mut report = ProbeReport {
        backend: BackendId::Terminal,
        available: false,
        version: None,
        protocol: None,
        checks: Vec::new(),
    };

    match cli
        .run_xdg_text(&argv(&["--print-id"]), PROBE_TIMEOUT)
        .await
    {
        Ok(text) => {
            let id = text.trim().to_string();
            checks.push(Check::pass(
                "xdg-terminal-exec on PATH",
                format!("resolves to {id} at {}", cli.xdg_bin().display()),
            ));
            report.version = Some(id);
        }
        Err(err) => {
            checks.push(Check::fail(
                "xdg-terminal-exec on PATH",
                err.to_string(),
                err.remedy().unwrap_or(XDG_INSTALL_REMEDY),
            ));
            report.checks = checks;
            return report;
        }
    }

    checks.push(hyprctl_check(cli).await);
    checks.push(wayland_display_check());

    report.available = checks.iter().all(|c| c.passed);
    report.checks = checks;
    report
}

async fn hyprctl_check(cli: &TerminalCli) -> Check {
    let name = "hyprctl answering";
    match cli
        .run_hypr_json(&argv(&["version", "-j"]), PROBE_TIMEOUT)
        .await
    {
        Ok(result) => {
            let version = result
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Check::pass(name, format!("hyprland {version}"))
        }
        Err(err) => Check::fail(
            name,
            err.to_string(),
            err.remedy().unwrap_or(HYPR_INSTALL_REMEDY),
        ),
    }
}

fn wayland_display_check() -> Check {
    wayland_display_check_for(std::env::var("WAYLAND_DISPLAY").ok())
}

fn wayland_display_check_for(value: Option<String>) -> Check {
    let name = "$WAYLAND_DISPLAY set";
    match value {
        Some(v) if !v.is_empty() => Check::pass(name, v),
        _ => Check::fail(
            name,
            "$WAYLAND_DISPLAY is empty or unset",
            "run mike from inside a Wayland session (Hyprland sets this automatically)",
        ),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn wayland_display_check_set_passes_unset_or_empty_fails() {
        assert!(wayland_display_check_for(Some("wayland-1".to_string())).passed);
        assert!(!wayland_display_check_for(Some(String::new())).passed);
        assert!(!wayland_display_check_for(None).passed);
    }
}
