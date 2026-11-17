use serde_json::json;

use super::report::{
    CostView, FileReport, Window, exposed_sessions, no_shell_arms, python_gaps, render_file_report,
    shell_lines,
};
use super::test_support::{arms, at, cut, primary};
use super::types::{
    Decision, ExposureRow, FILE_SHARE, Gaps, HarmTable, NO_SHELL_ARMS, ShellRow, ShellTable,
};

fn costs<'a>(forward: &'a [f64], control: &'a [f64]) -> CostView<'a> {
    CostView {
        forward,
        control,
        lane_forward: &[],
        lane_control: &[],
        p: 0.0123,
        lane_p: 1.0,
    }
}

#[test]
fn python_gaps_formats_like_json_dumps_with_sorted_keys() {
    assert_eq!(python_gaps(&Gaps::new()), "{}");
    let gaps = Gaps::from([("b".to_string(), 2), ("a".to_string(), 1)]);
    assert_eq!(python_gaps(&gaps), "{\"a\": 1, \"b\": 2}");
}

#[test]
fn exposed_sessions_counts_assigned_sessions_that_had_a_cut() {
    let assigned = arms(&[(1, "forward"), (2, "forward"), (3, "control")]);
    let cuts = [
        cut(primary(1), "w", "r", "d", "2026-11-01T10:00:00Z"),
        cut(primary(3), "w2", "r", "d", "2026-11-01T10:00:00Z"),
        cut(primary(9), "w3", "r", "d", "2026-11-01T10:00:00Z"),
    ];
    assert_eq!(exposed_sessions(&cuts, &assigned), (1, 1));
}

fn file_report_text(decisions: &[Decision], applied: bool, qualified: bool) -> String {
    let assigned = arms(&[(1, "forward"), (2, "control")]);
    let table = HarmTable::from([(
        (".md".to_string(), "forward".to_string()),
        ExposureRow {
            cuts: 40,
            reread: 2,
            edit_hidden: 3,
            exposures_with_hidden_edit: 3,
            exposures_with_reread: 2,
        },
    )]);
    let gaps = Gaps::from([("right_censored".to_string(), 4)]);
    let forward = [100.0, 300.0];
    let control = [250.0];
    let shell = vec![NO_SHELL_ARMS.to_string()];
    render_file_report(&FileReport {
        now: at("2026-11-15T12:34:56Z"),
        window: Window {
            start: "2026-11-08T12:00:00.000000+00:00",
            arms: &assigned,
            exposed: (1, 0),
        },
        costs: costs(&forward, &control),
        table: &table,
        gaps: &gaps,
        shell: &shell,
        decisions,
        applied,
        qualified,
    })
}

#[test]
fn file_report_states_window_arms_costs_table_and_blocked_widening() {
    let text = file_report_text(&[], false, false);
    for expected in [
        "# File-read compressor scorecard, 2026-11-15 12:34Z\n",
        "Window from 2026-11-08T12:00:00.000000+00:00. Sessions assigned: forward 1, control 1. Sessions with a read cut or candidate: forward 1, control 0.\n",
        "| forward | 2 | 200 | 0 | n/a |\n",
        "| control | 1 | 250 | 0 | n/a |\n",
        "Permutation test on log cost per turn: p=0.012; per subagent lane: p=1.000.\n",
        "| .md | forward | 40 | 2 | 3 |\n",
        "Evidence gaps: {\"right_censored\": 4}\n",
        "\nShell-output compression: no shell-output arms were recorded in the window.\n",
        "Decisions (not applied; run with --apply):\n- none\n",
        "\nAutomatic widening: blocked: missing, failed, stale or policy-mismatched task/recovery evidence\n",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
}

#[test]
fn file_report_lists_applied_decisions_and_qualified_widening() {
    let decisions = [Decision {
        action: "stop",
        key: FILE_SHARE,
        value: json!(0),
        why: "forward arm costs more per turn (p=0.010)".to_string(),
    }];
    let text = file_report_text(&decisions, true, true);
    assert!(
        text.contains("Decisions (applied):\n- stop: forward arm costs more per turn (p=0.010)\n")
    );
    assert!(text.ends_with("\nAutomatic widening: qualified\n"));
}

#[test]
fn shell_lines_report_rates_and_the_forward_minus_control_gap() {
    let assigned = arms(&[(1, "forward"), (2, "control")]);
    let table = ShellTable {
        forward: ShellRow {
            cuts: 40,
            needed_later: 6,
        },
        control: ShellRow {
            cuts: 40,
            needed_later: 1,
        },
        by_command: [
            (
                "rg".to_string(),
                ShellRow {
                    cuts: 30,
                    needed_later: 6,
                },
            ),
            (
                "cargo test".to_string(),
                ShellRow {
                    cuts: 50,
                    needed_later: 1,
                },
            ),
        ]
        .into_iter()
        .collect(),
        left_whole_now: ShellRow {
            cuts: 12,
            needed_later: 3,
        },
    };
    let window = Window {
        start: "2026-11-08T12:00:00.000000+00:00",
        arms: &assigned,
        exposed: (1, 1),
    };
    let lines = shell_lines(&window, &costs(&[], &[]), &table, &Gaps::new()).join("\n");
    assert!(lines.starts_with("## Shell-output compression\n\nWindow from"));
    assert!(lines.contains("Sessions with a shell cut or candidate: forward 1, control 1."));
    assert!(lines.contains("| forward | 40 | 6 | 15.0% |"));
    assert!(lines.contains("| control | 40 | 1 | 2.5% |"));
    assert!(lines.contains("\n\nForward minus control: +12.5%.\n"));
    assert!(lines.contains("| `rg` | 30 | 6 | 20.0% |\n| `cargo test` | 50 | 1 | 2.0% |"));
    assert!(lines.contains("Not listed: 12 cuts of commands the current policy leaves whole"));
    assert!(lines.contains("3 needed later (25.0%)."));
    assert!(lines.ends_with("Evidence gaps: {}"));
}

#[test]
fn no_shell_arms_is_one_line() {
    assert_eq!(no_shell_arms(), vec![NO_SHELL_ARMS.to_string()]);
}
