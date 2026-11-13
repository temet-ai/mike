use super::gate::{GateReport, NOT_RUN, RUNNING};
use super::gate_failures::summary;

const SHA_SHOWN: usize = 10;

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(SHA_SHOWN)]
}

pub fn text(report: &GateReport) -> String {
    if let Some(why) = &report.refused {
        return format!("refused: {why}");
    }
    let unpushed = if report.pushed {
        ""
    } else {
        ", which is not pushed yet"
    };
    match report.how.as_str() {
        super::check_run::completion::RECORDING => format!(
            "`{}` has ended at commit {} and its immutable result is being recorded. The command is not running again; mike mails the verdict after it is safely stored",
            report.command,
            short(&report.commit)
        ),
        super::check_run::completion::BLOCKED => format!(
            "`{}` has ended, but recording its result is blocked. Read gate status for the persistence error and captured output location; this is not a completed gate receipt",
            report.command
        ),
        RUNNING => format!(
            "`{}` is running at commit {}{unpushed}, for {} s so far{}, in {}. mike mails the seat that asked the verdict when it ends; the gate reads an isolated checkout of the captured commit; seats can keep editing their shared checkout",
            report.command,
            short(&report.commit),
            report.seconds,
            report
                .started_at
                .map(|at| format!(" (since {})", at.format("%H:%M:%SZ")))
                .unwrap_or_default(),
            report.checkout
        ),
        NOT_RUN => format!(
            "`{}` has not run at commit {}{unpushed} and is not running now",
            report.command,
            short(&report.commit)
        ),
        how => {
            let mut lines = vec![format!(
                "`{}` {} at commit {}{unpushed} ({how}, {} s, in {})",
                report.command,
                if !report.source_verified {
                    "has unverified source (no commit verdict)"
                } else if report.passed {
                    "passed"
                } else if report.exit_code.is_none() {
                    "did not complete (no exit code)"
                } else {
                    "failed"
                },
                short(&report.commit),
                report.seconds,
                report.checkout
            )];
            if !report.passed {
                lines.push(summary(&report.failure_inventory, &report.output_file));
            }
            lines.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(how: &str) -> GateReport {
        GateReport {
            seat: "acceptance".into(),
            command: "make check".into(),
            commit: "0123456789abcdef".into(),
            pushed: true,
            how: how.into(),
            passed: false,
            exit_code: Some(2),
            seconds: 300,
            checkout: "/srv/project-mike-gate".into(),
            output_file: "/srv/project-mike-gate/.mike-gate-output.log".into(),
            output_tail: "fmt-check failed\nmake: *** [Makefile:55] Error 1".into(),
            failure_inventory: Vec::new(),
            source_verified: true,
            refused: None,
            started_at: "2026-10-02T01:20:00Z".parse().ok(),
        }
    }

    #[test]
    fn a_failed_gate_names_the_commit_the_checkout_and_where_all_the_output_is() {
        assert_eq!(
            text(&report("fresh")),
            "`make check` failed at commit 0123456789 (fresh, 300 s, in /srv/project-mike-gate)\nFailing-test inventory unparsed: no recognized test IDs in captured output.\nFull captured output: /srv/project-mike-gate/.mike-gate-output.log"
        );
        let passed = GateReport {
            passed: true,
            exit_code: Some(0),
            pushed: false,
            ..report("reused")
        };
        assert_eq!(
            text(&passed),
            "`make check` passed at commit 0123456789, which is not pushed yet (reused, 300 s, in /srv/project-mike-gate)"
        );
    }

    #[test]
    fn a_gate_still_running_says_seats_keep_working() {
        let running = text(&report("running"));
        assert!(running.starts_with(
            "`make check` is running at commit 0123456789, for 300 s so far (since 01:20:00Z), in /srv/project-mike-gate."
        ), "{running}");
        assert!(running.ends_with(
            "the gate reads an isolated checkout of the captured commit; seats can keep editing their shared checkout"
        ));
        assert_eq!(
            text(&report("not run")),
            "`make check` has not run at commit 0123456789 and is not running now"
        );
        let refused = GateReport {
            refused: Some("only the acceptance seat starts the full gate".into()),
            ..report("refused")
        };
        assert_eq!(
            text(&refused),
            "refused: only the acceptance seat starts the full gate"
        );
    }

    #[test]
    fn an_ended_gate_waiting_on_persistence_never_claims_it_is_still_executing() {
        let recording = text(&report(super::super::check_run::completion::RECORDING));
        assert!(recording.contains("has ended"));
        assert!(recording.contains("being recorded"));
        let blocked = text(&report(super::super::check_run::completion::BLOCKED));
        assert!(blocked.contains("not a completed gate receipt"));
    }
}
