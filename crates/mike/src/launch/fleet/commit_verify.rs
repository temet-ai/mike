use super::check::{short, state_for};
use super::check_run::baseline::diagnostics;
use super::definition::FleetDefinition;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;

#[derive(Debug, Clone, Copy)]
pub struct Verifier<'a> {
    pub persist: Option<&'a PersistHandle>,
    pub run: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    NoCheck(String),
    Unchecked(String),
    FailsInMine(Vec<String>),
    FailsElsewhere(String),
    Proven(String),
}

pub struct Verification<'a> {
    pub def: &'a FleetDefinition,
    pub tree: &'a str,
    pub paths: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub text: String,
    pub warning: Option<String>,
}

pub async fn verify(verifier: Verifier<'_>, v: Verification<'_>) -> Verified {
    let evidence = gather(verifier, v.def, v.tree, v.paths).await;
    let warning = match &evidence {
        Evidence::FailsInMine(lines) => Some(format!(
            "the check fails at the content of this commit in the files just committed:\n{}",
            lines.join("\n")
        )),
        _ => None,
    };
    Verified {
        text: describe(&evidence),
        warning,
    }
}

async fn gather(
    verifier: Verifier<'_>,
    def: &FleetDefinition,
    tree: &str,
    paths: &[String],
) -> Evidence {
    if !def.check.declared() {
        return Evidence::NoCheck("no check command is declared".to_string());
    }
    if !super::operation::reusable(def) {
        return Evidence::Unchecked("this operation has no reusable-result contract; the committed revision needs its own check".into());
    }
    let Some(persist) = verifier.persist else {
        return Evidence::NoCheck("no database to read check results from".to_string());
    };
    let state = match state_for(def, tree).await {
        Ok(state) => state,
        Err(why) => return Evidence::Unchecked(why),
    };
    match persist.find_tree_check(verifier.run, &state).await {
        Ok(Some(row)) => judge(def, &row, paths),
        Ok(None) => Evidence::Unchecked(format!(
            "mike runs `{}` at this commit (state {}) in its clean checkout once it lands, and records the verdict against it; fleet_check reads it",
            def.check.command.trim(),
            short(&state)
        )),
        Err(err) => Evidence::Unchecked(format!("check results could not be read: {err}")),
    }
}

pub(super) fn judge(def: &FleetDefinition, row: &TreeCheckRow, paths: &[String]) -> Evidence {
    let state = short(&row.state).to_string();
    if row.passed() {
        return Evidence::Proven(state);
    }
    let mine: Vec<String> = diagnostics(def, row)
        .into_iter()
        .filter(|d| paths.contains(&d.path))
        .map(|d| format!("- {}:{} {}", d.path, d.line, d.text))
        .collect();
    if mine.is_empty() {
        Evidence::FailsElsewhere(state)
    } else {
        Evidence::FailsInMine(mine)
    }
}

pub fn describe(evidence: &Evidence) -> String {
    match evidence {
        Evidence::NoCheck(why) => format!("not proven; {why}"),
        Evidence::Unchecked(why) => format!("not yet proven; {why}"),
        Evidence::FailsInMine(lines) => format!(
            "not proven: the check fails in these files at the content of this commit ({} line(s)); committed all the same",
            lines.len()
        ),
        Evidence::FailsElsewhere(state) => format!(
            "not proven: the check fails at state {state}, the content of this commit, but not in these files"
        ),
        Evidence::Proven(state) => format!(
            "proven: the check passed at state {state}, the content of this commit under the check's paths"
        ),
    }
}
