use std::collections::HashMap;

use serde_json::{Value, json};

use super::decide::{Costs, Settings, decide, decide_shell};
use super::types::{
    Decision, ExposureRow, FILE_PROTECTED, FILE_SHARE, HarmTable, SHELL_SHARE, ShellRow, ShellTable,
};

fn settings(pairs: &[(&str, Value)]) -> Settings {
    Settings(
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<HashMap<_, _>>(),
    )
}

fn file_settings() -> Settings {
    settings(&[
        (FILE_PROTECTED, json!(["**/CLAUDE.md"])),
        (FILE_SHARE, json!(50)),
    ])
}

fn table(forward_hidden: u64, control_hidden: u64, cuts: u64) -> HarmTable {
    let row = |hidden| ExposureRow {
        cuts,
        reread: 0,
        edit_hidden: hidden,
        exposures_with_hidden_edit: hidden,
        exposures_with_reread: 0,
    };
    HarmTable::from([
        (
            (".md".to_string(), "forward".to_string()),
            row(forward_hidden),
        ),
        (
            (".md".to_string(), "control".to_string()),
            row(control_hidden),
        ),
    ])
}

fn run(table: &HarmTable, forward: f64, control: f64, p: f64, qualified: bool) -> Vec<Decision> {
    let (f, c) = (vec![forward; 40], vec![control; 40]);
    let costs = Costs {
        forward: &f,
        control: &c,
        p,
    };
    decide(table, &costs, &file_settings(), qualified)
}

fn stop(key: &'static str, why: &str) -> Decision {
    Decision {
        action: "stop",
        key,
        value: json!(0),
        why: why.to_string(),
    }
}

#[test]
fn decide_type_harmed_beyond_the_control_is_protected() {
    let decisions = decide(
        &table(4, 1, 40),
        &Costs {
            forward: &[],
            control: &[],
            p: 1.0,
        },
        &file_settings(),
        false,
    );
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].action, "protect");
    assert_eq!(decisions[0].value, json!(["**/CLAUDE.md", "**/*.md"]));
    assert_eq!(
        decisions[0].why,
        ".md: edits needing hidden lines +7.5% over control"
    );
}

#[test]
fn decide_harm_matching_the_control_is_not_acted_on() {
    assert!(run(&table(3, 3, 40), 100.0, 100.0, 1.0, false).is_empty());
}

#[test]
fn decide_too_few_reads_is_not_acted_on() {
    assert!(run(&table(10, 0, 10), 100.0, 100.0, 1.0, false).is_empty());
}

#[test]
fn decide_none_type_is_never_protected() {
    let mut harmed = HarmTable::new();
    for (arm, hidden) in [("forward", 10), ("control", 0)] {
        harmed.insert(
            ("(none)".to_string(), arm.to_string()),
            ExposureRow {
                cuts: 40,
                exposures_with_hidden_edit: hidden,
                ..ExposureRow::default()
            },
        );
    }
    assert!(run(&harmed, 100.0, 100.0, 1.0, false).is_empty());
}

#[test]
fn decide_already_protected_type_is_not_protected_again() {
    let covered = settings(&[
        (FILE_PROTECTED, json!(["**/*.md"])),
        (FILE_SHARE, json!(50)),
    ]);
    let (f, c) = (vec![100.0; 40], vec![100.0; 40]);
    let costs = Costs {
        forward: &f,
        control: &c,
        p: 1.0,
    };
    assert!(decide(&table(4, 1, 40), &costs, &covered, false).is_empty());
}

#[test]
fn decide_dearer_forward_arm_stops_forwarding() {
    let decisions = run(&HarmTable::new(), 200.0, 100.0, 0.01, false);
    assert_eq!(
        decisions,
        vec![stop(
            FILE_SHARE,
            "forward arm costs more per turn (p=0.010)"
        )]
    );
}

#[test]
fn decide_lower_cost_cannot_widen_without_qualifying_evidence() {
    assert!(run(&HarmTable::new(), 80.0, 100.0, 0.01, false).is_empty());
}

#[test]
fn decide_lower_cost_widens_once_the_evidence_qualifies() {
    let decisions = run(&HarmTable::new(), 80.0, 100.0, 0.01, true);
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].action, "widen");
    assert_eq!(decisions[0].value, json!(90));
    assert_eq!(
        decisions[0].why,
        "forward arm costs less per turn (p=0.010)"
    );
}

#[test]
fn decide_unclear_difference_changes_nothing() {
    assert!(run(&HarmTable::new(), 80.0, 100.0, 0.4, true).is_empty());
}

#[test]
fn decide_protection_prevents_widening_in_the_same_batch() {
    let decisions = run(&table(4, 0, 40), 80.0, 100.0, 0.01, true);
    let actions: Vec<&str> = decisions.iter().map(|d| d.action).collect();
    assert_eq!(actions, vec!["protect"]);
}

#[test]
fn decide_share_already_zero_proposes_no_stop() {
    let (f, c) = (vec![200.0; 40], vec![100.0; 40]);
    let costs = Costs {
        forward: &f,
        control: &c,
        p: 0.01,
    };
    let off = settings(&[(FILE_SHARE, json!(0))]);
    assert!(decide(&HarmTable::new(), &costs, &off, true).is_empty());
}

fn shell_table(forward: u64, control: u64, cuts: u64) -> ShellTable {
    ShellTable {
        forward: ShellRow {
            cuts,
            needed_later: forward,
        },
        control: ShellRow {
            cuts,
            needed_later: control,
        },
    }
}

fn shell(forward: f64, control: f64, count: usize, p: f64, table: &ShellTable) -> Vec<Decision> {
    let (f, c) = (vec![forward; count], vec![control; count]);
    let costs = Costs {
        forward: &f,
        control: &c,
        p,
    };
    decide_shell(&costs, table, &settings(&[(SHELL_SHARE, json!(50))]))
}

#[test]
fn decide_shell_dearer_forward_arm_stops_shell_forwarding() {
    assert_eq!(
        shell(200.0, 100.0, 40, 0.01, &ShellTable::default()),
        vec![stop(
            SHELL_SHARE,
            "forward arm costs more per turn (p=0.010)"
        )]
    );
}

#[test]
fn decide_shell_hidden_output_needed_beyond_the_control_stops_it() {
    let decisions = shell(0.0, 0.0, 0, 1.0, &shell_table(6, 1, 40));
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].action, "stop");
    assert_eq!(decisions[0].value, json!(0));
    assert!(
        decisions[0]
            .why
            .contains("needed later +12.5% over control")
    );
}

#[test]
fn decide_shell_both_reasons_are_joined() {
    let decisions = shell(200.0, 100.0, 40, 0.01, &shell_table(6, 1, 40));
    assert_eq!(
        decisions[0].why,
        "forward arm costs more per turn (p=0.010); hidden shell output needed later +12.5% over control"
    );
}

#[test]
fn decide_shell_without_evidence_changes_nothing() {
    let none = ShellTable::default();
    assert!(shell(80.0, 100.0, 40, 0.01, &none).is_empty());
    assert!(shell(200.0, 100.0, 40, 0.4, &none).is_empty());
    assert!(shell(200.0, 100.0, 5, 0.01, &none).is_empty());
    assert!(shell(0.0, 0.0, 0, 1.0, &shell_table(3, 3, 40)).is_empty());
    assert!(shell(0.0, 0.0, 0, 1.0, &shell_table(10, 0, 10)).is_empty());
}

#[test]
fn decide_shell_share_already_zero_proposes_nothing() {
    let (f, c) = (vec![200.0; 40], vec![100.0; 40]);
    let costs = Costs {
        forward: &f,
        control: &c,
        p: 0.01,
    };
    let off = settings(&[(SHELL_SHARE, json!(0))]);
    assert!(decide_shell(&costs, &shell_table(6, 1, 40), &off).is_empty());
    assert!(decide_shell(&costs, &shell_table(6, 1, 40), &Settings::default()).is_empty());
}
