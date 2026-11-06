use std::fmt::Write;

use super::{Report, Status};

fn label(status: Status) -> &'static str {
    match status {
        Status::Ok => "ok",
        Status::Warn => "warn",
        Status::Fail => "FAIL",
    }
}

pub(crate) fn text(report: &Report) -> String {
    titled("mike doctor", report)
}

pub(crate) fn titled(title: &str, report: &Report) -> String {
    let mut out = format!("{title} ({})\n\n", report.version);
    let width = report
        .checks
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(0);
    for check in &report.checks {
        let _ = writeln!(
            out,
            "{:<5} {:<width$}  {}",
            label(check.status),
            check.name,
            check.detail
        );
        if let (Some(fix), true) = (&check.fix, check.status != Status::Ok) {
            let _ = writeln!(out, "{:<5} {:<width$}  fix: {fix}", "", "");
        }
    }
    let _ = writeln!(
        out,
        "\n{} ok, {} warnings, {} failures",
        report.ok, report.warn, report.fail
    );
    out
}

#[cfg(test)]
pub(crate) fn json(report: &Report) -> String {
    serde_json::to_string_pretty(report)
        .unwrap_or_else(|e| format!("{{\"passed\":false,\"error\":\"json encoding failed: {e}\"}}"))
}
