use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use super::stats::median;
use super::types::{
    ARM_CONTROL, ARM_FORWARD, Decision, FILE_PROTECTED, FILE_SHARE, HARM_LIMIT, HarmTable,
    MIN_READS_PER_ARM, MIN_SESSIONS_PER_ARM, P_LIMIT, SHELL_SHARE, ShellTable, WIDE_SHARE,
};

#[derive(Debug, Clone, Default)]
pub(super) struct Settings(pub(super) HashMap<String, Value>);

impl Settings {
    pub(super) fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    fn number(&self, key: &str) -> f64 {
        self.get(key).and_then(Value::as_f64).unwrap_or(0.0)
    }

    fn strings(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub(super) struct Costs<'a> {
    pub(super) forward: &'a [f64],
    pub(super) control: &'a [f64],
    pub(super) p: f64,
}

impl Costs<'_> {
    fn decisive(&self) -> bool {
        self.forward.len() >= MIN_SESSIONS_PER_ARM
            && self.control.len() >= MIN_SESSIONS_PER_ARM
            && self.p < P_LIMIT
    }

    fn forward_dearer(&self) -> bool {
        median(self.forward) > median(self.control)
    }
}

fn exposure_rate(table: &HarmTable, ext: &str, arm: &str) -> Option<(u64, f64)> {
    let row = table.get(&(ext.to_string(), arm.to_string()))?;
    (row.cuts > 0).then(|| {
        (
            row.cuts,
            row.exposures_with_hidden_edit as f64 / row.cuts as f64,
        )
    })
}

fn protect_decisions(table: &HarmTable, settings: &Settings) -> Vec<Decision> {
    let mut protected = settings.strings(FILE_PROTECTED);
    let mut decisions = Vec::new();
    let extensions: BTreeSet<&str> = table.keys().map(|(ext, _)| ext.as_str()).collect();
    for ext in extensions {
        let forward = exposure_rate(table, ext, ARM_FORWARD);
        let control = exposure_rate(table, ext, ARM_CONTROL);
        let (Some((f_cuts, f_rate)), Some((c_cuts, c_rate))) = (forward, control) else {
            continue;
        };
        if f_cuts < MIN_READS_PER_ARM || c_cuts < MIN_READS_PER_ARM || ext == "(none)" {
            continue;
        }
        let excess = f_rate - c_rate;
        let glob = format!("**/*{ext}");
        if excess >= HARM_LIMIT && !protected.contains(&glob) {
            protected.push(glob);
            decisions.push(Decision {
                action: "protect",
                key: FILE_PROTECTED,
                value: Value::from(protected.clone()),
                why: format!(
                    "{ext}: edits needing hidden lines {} over control",
                    signed_percent(excess)
                ),
            });
        }
    }
    decisions
}

pub(super) fn signed_percent(share: f64) -> String {
    format!("{:+.1}%", share * 100.0)
}

pub(super) fn decide(
    table: &HarmTable,
    costs: &Costs,
    settings: &Settings,
    qualified: bool,
) -> Vec<Decision> {
    let mut decisions = protect_decisions(table, settings);
    if !costs.decisive() {
        return decisions;
    }
    let share = settings.number(FILE_SHARE);
    let dearer = costs.forward_dearer();
    if dearer && share != 0.0 {
        decisions.push(Decision {
            action: "stop",
            key: FILE_SHARE,
            value: Value::from(0),
            why: format!("forward arm costs more per turn (p={:.3})", costs.p),
        });
    } else if !dearer && share < WIDE_SHARE && decisions.is_empty() && qualified {
        decisions.push(Decision {
            action: "widen",
            key: FILE_SHARE,
            value: Value::from(WIDE_SHARE as i64),
            why: format!("forward arm costs less per turn (p={:.3})", costs.p),
        });
    }
    decisions
}

pub(super) fn decide_shell(
    costs: &Costs,
    table: &ShellTable,
    settings: &Settings,
) -> Vec<Decision> {
    if settings.number(SHELL_SHARE) == 0.0 {
        return Vec::new();
    }
    let mut reasons = Vec::new();
    if costs.decisive() && costs.forward_dearer() {
        reasons.push(format!(
            "forward arm costs more per turn (p={:.3})",
            costs.p
        ));
    }
    let (f, c) = (&table.forward, &table.control);
    if f.cuts >= MIN_READS_PER_ARM && c.cuts >= MIN_READS_PER_ARM {
        let excess = f.needed_later as f64 / f.cuts as f64 - c.needed_later as f64 / c.cuts as f64;
        if excess >= HARM_LIMIT {
            reasons.push(format!(
                "hidden shell output needed later {} over control",
                signed_percent(excess)
            ));
        }
    }
    if reasons.is_empty() {
        return Vec::new();
    }
    vec![Decision {
        action: "stop",
        key: SHELL_SHARE,
        value: Value::from(0),
        why: reasons.join("; "),
    }]
}
