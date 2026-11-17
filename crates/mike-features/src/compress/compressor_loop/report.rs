use chrono::{DateTime, Utc};

use super::decide::signed_percent;
use super::stats::median;
use super::types::{
    ARM_CONTROL, ARM_FORWARD, Arms, Cut, Decision, Gaps, HarmTable, NO_SHELL_ARMS, ShellRow,
    ShellTable,
};

pub(super) struct CostView<'a> {
    pub(super) forward: &'a [f64],
    pub(super) control: &'a [f64],
    pub(super) lane_forward: &'a [f64],
    pub(super) lane_control: &'a [f64],
    pub(super) p: f64,
    pub(super) lane_p: f64,
}

pub(super) struct Window<'a> {
    pub(super) start: &'a str,
    pub(super) arms: &'a Arms,
    pub(super) exposed: (usize, usize),
}

pub(super) struct FileReport<'a> {
    pub(super) now: DateTime<Utc>,
    pub(super) window: Window<'a>,
    pub(super) costs: CostView<'a>,
    pub(super) table: &'a HarmTable,
    pub(super) gaps: &'a Gaps,
    pub(super) shell: &'a [String],
    pub(super) decisions: &'a [Decision],
    pub(super) applied: bool,
    pub(super) qualified: bool,
}

pub(super) fn exposed_sessions(cuts: &[Cut], arms: &Arms) -> (usize, usize) {
    let exposed: std::collections::HashSet<i64> =
        cuts.iter().map(|cut| cut.who.session_id).collect();
    let count = |arm: &str| {
        arms.iter()
            .filter(|(session, assigned)| assigned.as_str() == arm && exposed.contains(session))
            .count()
    };
    (count(ARM_FORWARD), count(ARM_CONTROL))
}

pub(super) fn python_gaps(gaps: &Gaps) -> String {
    let fields: Vec<String> = gaps
        .iter()
        .map(|(name, count)| format!("\"{name}\": {count}"))
        .collect();
    format!("{{{}}}", fields.join(", "))
}

fn median_text(values: &[f64]) -> String {
    median(values).map_or_else(
        || "n/a".to_string(),
        |middle| format!("{}", middle.round_ties_even() as i64),
    )
}

fn assigned(arms: &Arms, arm: &str) -> usize {
    arms.values()
        .filter(|assigned| assigned.as_str() == arm)
        .count()
}

fn session_line(window: &Window, noun: &str) -> String {
    format!(
        "Window from {}. Sessions assigned: forward {}, control {}. \
         Sessions with a {noun} cut or candidate: forward {}, control {}.",
        window.start,
        assigned(window.arms, ARM_FORWARD),
        assigned(window.arms, ARM_CONTROL),
        window.exposed.0,
        window.exposed.1,
    )
}

fn cost_table(costs: &CostView) -> Vec<String> {
    let mut lines = vec![
        "| Arm | Sessions with turns | Median cost per turn | Subagent lanes | Median cost per lane |"
            .to_string(),
        "|---|---:|---:|---:|---:|".to_string(),
    ];
    for (name, turns, lanes) in [
        (ARM_FORWARD, costs.forward, costs.lane_forward),
        (ARM_CONTROL, costs.control, costs.lane_control),
    ] {
        lines.push(format!(
            "| {name} | {} | {} | {} | {} |",
            turns.len(),
            median_text(turns),
            lanes.len(),
            median_text(lanes),
        ));
    }
    lines
}

fn p_line(costs: &CostView) -> String {
    format!(
        "Permutation test on log cost per turn: p={:.3}; per subagent lane: p={:.3}.",
        costs.p, costs.lane_p
    )
}

pub(super) fn render_file_report(report: &FileReport) -> String {
    let mut lines = vec![
        format!(
            "# File-read compressor scorecard, {}Z",
            report.now.format("%Y-%m-%d %H:%M")
        ),
        String::new(),
        session_line(&report.window, "read"),
        String::new(),
    ];
    lines.extend(cost_table(&report.costs));
    lines.extend([
        String::new(),
        p_line(&report.costs),
        "Cost counts billed replies only (200s to /v1/messages), usage from the stored body when the column is empty."
            .to_string(),
        String::new(),
        "| File type | Arm | Reads cut | Re-read later | Edit needing hidden lines |".to_string(),
        "|---|---|---:|---:|---:|".to_string(),
    ]);
    for ((ext, arm), row) in report.table {
        lines.push(format!(
            "| {ext} | {arm} | {} | {} | {} |",
            row.cuts, row.reread, row.edit_hidden
        ));
    }
    lines.extend([
        String::new(),
        "Measure: lane-aware-followups-v2; unique follow-up tools within 30 minutes of exposure and receipt.".to_string(),
        "Later reads are associations, not proven recovery. Unknown lanes/results and right-censored exposures are excluded.".to_string(),
        format!("Evidence gaps: {}", python_gaps(report.gaps)),
    ]);
    if !report.shell.is_empty() {
        lines.push(String::new());
        lines.extend(report.shell.iter().cloned());
    }
    lines.push(String::new());
    lines.push(format!(
        "Decisions{}:",
        if report.applied {
            " (applied)"
        } else {
            " (not applied; run with --apply)"
        }
    ));
    if report.decisions.is_empty() {
        lines.push("- none".to_string());
    }
    lines.extend(
        report
            .decisions
            .iter()
            .map(|decision| format!("- {}: {}", decision.action, decision.why)),
    );
    let mut text = lines.join("\n");
    text.push('\n');
    text.push_str("\nAutomatic widening: ");
    text.push_str(if report.qualified {
        "qualified"
    } else {
        "blocked: missing, failed, stale or policy-mismatched task/recovery evidence"
    });
    text.push('\n');
    text
}

fn rate(row: &ShellRow) -> String {
    if row.cuts == 0 {
        return "n/a".to_string();
    }
    format!("{:.1}%", row.needed_later as f64 / row.cuts as f64 * 100.0)
}

const COMMAND_ROWS: usize = 20;

fn command_lines(table: &ShellTable) -> Vec<String> {
    if table.by_command.is_empty() {
        return Vec::new();
    }
    let mut rows: Vec<(&String, &ShellRow)> = table.by_command.iter().collect();
    rows.sort_by(|(a_name, a), (b_name, b)| {
        b.needed_later
            .cmp(&a.needed_later)
            .then(b.cuts.cmp(&a.cuts))
            .then(a_name.cmp(b_name))
    });
    let mut lines = vec![
        String::new(),
        "Where cuts miss, by command (both arms):".to_string(),
        String::new(),
        "| Command | Shell outputs cut | Hidden part needed later | Rate |".to_string(),
        "|---|---:|---:|---:|".to_string(),
    ];
    for (name, row) in rows.iter().take(COMMAND_ROWS) {
        lines.push(format!(
            "| `{name}` | {} | {} | {} |",
            row.cuts,
            row.needed_later,
            rate(row)
        ));
    }
    if rows.len() > COMMAND_ROWS {
        let rest = &rows[COMMAND_ROWS..];
        let cuts: u64 = rest.iter().map(|(_, row)| row.cuts).sum();
        let needed: u64 = rest.iter().map(|(_, row)| row.needed_later).sum();
        lines.push(format!(
            "| {} other commands | {cuts} | {needed} | {} |",
            rest.len(),
            rate(&ShellRow {
                cuts,
                needed_later: needed
            })
        ));
    }
    lines
}

pub(super) fn shell_lines(
    window: &Window,
    costs: &CostView,
    table: &ShellTable,
    gaps: &Gaps,
) -> Vec<String> {
    let mut lines = vec![
        "## Shell-output compression".to_string(),
        String::new(),
        session_line(window, "shell"),
        String::new(),
    ];
    lines.extend(cost_table(costs));
    lines.extend([
        String::new(),
        p_line(costs),
        String::new(),
        "| Arm | Shell outputs cut | Hidden part needed later | Rate |".to_string(),
        "|---|---:|---:|---:|".to_string(),
    ]);
    for (arm, row) in [(ARM_FORWARD, &table.forward), (ARM_CONTROL, &table.control)] {
        lines.push(format!(
            "| {arm} | {} | {} | {} |",
            row.cuts,
            row.needed_later,
            rate(row)
        ));
    }
    let (f, c) = (&table.forward, &table.control);
    if f.cuts > 0 && c.cuts > 0 {
        let excess = f.needed_later as f64 / f.cuts as f64 - c.needed_later as f64 / c.cuts as f64;
        lines.push(format!(
            "\nForward minus control: {}.",
            signed_percent(excess)
        ));
    }
    lines.extend(command_lines(table));
    lines.extend([
        String::new(),
        "Measure: a distinctive token (path, Rust path, snake_case name, hash of 8+ characters) in the hidden lines,".to_string(),
        "absent from the kept lines and the cut's own command, used in a later tool call's input in the same lane".to_string(),
        "within 30 minutes of exposure and receipt. An association, not proven recovery. Right-censored cuts are excluded.".to_string(),
        format!("Evidence gaps: {}", python_gaps(gaps)),
    ]);
    lines
}

pub(super) fn no_shell_arms() -> Vec<String> {
    vec![NO_SHELL_ARMS.to_string()]
}
