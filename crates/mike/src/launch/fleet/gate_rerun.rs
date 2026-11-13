use crate::store::persistence::fleet_repo::FleetPersist as _;
use regex::Regex;

use super::definition::{FleetDefinition, GateDefinition};
use super::gate::{Target, ledger};
use crate::store::persistence::PersistHandle;

pub const RERUN_SUFFIX: &str = "#rerun";
const PLACEHOLDER: &str = "{test}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Rerun {
    pub(super) test: String,
    pub(super) passed: bool,
}

pub(super) fn rerun_ledger(run: &str) -> String {
    format!("{}{RERUN_SUFFIX}", ledger(run))
}

pub(super) fn named(pattern: &str, output: &str) -> Vec<String> {
    let Ok(regex) = Regex::new(pattern) else {
        return Vec::new();
    };
    let mut found: Vec<String> = Vec::new();
    for captures in regex.captures_iter(output) {
        let name = captures
            .name("test")
            .or_else(|| captures.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|name| !name.is_empty());
        if let Some(name) = name
            && !found.contains(&name)
        {
            found.push(name);
        }
    }
    found
}

pub(super) fn rerun_command(template: &str, test: &str) -> String {
    let quoted = format!("'{}'", test.replace('\'', "'\\''"));
    template.replace(PLACEHOLDER, &quoted)
}

pub(super) fn configured(gate: &GateDefinition) -> bool {
    !gate.timeout_pattern.trim().is_empty() && gate.rerun_command.contains(PLACEHOLDER)
}

pub(super) fn rerun_state(commit: &str, test: &str) -> String {
    format!("{commit}:{test}")
}

pub(super) async fn rerun_timeouts(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    target: &Target,
    output: &str,
) -> Vec<Rerun> {
    if !configured(&def.gate) {
        return Vec::new();
    }
    let mut outcomes = Vec::new();
    for test in named(&def.gate.timeout_pattern, output) {
        let passed = rerun_once(persist, def, run, target, &test).await;
        outcomes.push(Rerun { test, passed });
    }
    outcomes
}

async fn rerun_once(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    target: &Target,
    test: &str,
) -> bool {
    let ledger = rerun_ledger(run);
    let state = rerun_state(&target.commit, test);
    let command = rerun_command(&def.gate.rerun_command, test);
    if let Ok(Some(earlier)) = persist.fleet().find_tree_check(&ledger, &state).await
        && earlier.command == command
        && earlier.state_after.as_deref() == Some(state.as_str())
    {
        return earlier.passed();
    }
    let Ok(parent) = super::managed_command::current() else {
        return false;
    };
    let Ok(identity) = parent.authority.durable() else {
        return false;
    };
    let Ok(managed) = super::managed_command::begin(
        identity,
        super::managed_command::Receipt {
            ledger: &ledger,
            state: &state,
            command: &command,
            seat: "mike",
        },
        parent.private_host_root,
        parent.tracker,
    )
    .await
    else {
        return false;
    };
    let id = managed.id;
    let owner = managed.context.owner.clone();
    let tracker = managed.context.tracker.clone();
    let mut focused = def.clone();
    focused.gate.command = command;
    let (mut code, mut output, verified) = super::managed_command::scope(
        managed.context,
        super::gate::execute_verified(&focused, target, run),
    )
    .await;
    owner.phase(super::owned_execution::Phase::RecordingReceipt, None);
    let cancelled = super::managed_command::normalize_cancellation(&owner, &mut code, &mut output);
    let captured = super::check_run::completion::Captured {
        id,
        exit_code: if cancelled { None } else { code },
        output,
        state_after: if cancelled {
            "cancelled:unverified".into()
        } else if verified {
            state
        } else {
            "unverified-source".into()
        },
    };
    match super::check_run::completion::record(persist, &tracker, &ledger, &def.repo, captured)
        .await
    {
        Ok(_) => managed.registration.receipt_recorded(),
        Err(why) => {
            managed.registration.receipt_blocked(why);
            return false;
        }
    }

    !cancelled && code == Some(0)
}

pub(super) fn rerun_lines(reruns: &[Rerun]) -> Vec<String> {
    reruns
        .iter()
        .map(|r| {
            format!(
                "- {} timed out in the run and {}",
                r.test,
                if r.passed {
                    "passed alone"
                } else {
                    "failed alone"
                }
            )
        })
        .collect()
}

pub(super) fn only_timeouts_that_pass(reruns: &[Rerun]) -> bool {
    !reruns.is_empty() && reruns.iter().all(|r| r.passed)
}

pub(super) fn cleared(gate: &GateDefinition, output: &str, reruns: &[Rerun]) -> bool {
    if !configured(gate) || gate.failure_pattern.trim().is_empty() {
        return false;
    }
    let timeouts = named(&gate.timeout_pattern, output);
    let failures = named(&gate.failure_pattern, output);
    !timeouts.is_empty()
        && failures.iter().all(|f| timeouts.contains(f))
        && timeouts
            .iter()
            .all(|t| reruns.iter().any(|r| &r.test == t && r.passed))
}

pub(super) async fn recorded_reruns(
    persist: &PersistHandle,
    gate: &GateDefinition,
    (run, commit): (&str, &str),
    output: &str,
) -> Vec<Rerun> {
    let ledger = rerun_ledger(run);
    let mut found = Vec::new();
    for test in named(&gate.timeout_pattern, output) {
        let state = rerun_state(commit, &test);
        let command = rerun_command(&gate.rerun_command, &test);
        let passed = matches!(
            persist.fleet().find_tree_check(&ledger, &state).await,
            Ok(Some(row)) if row.passed() && row.command == command && row.state_after.as_deref() == Some(state.as_str())
        );
        found.push(Rerun { test, passed });
    }
    found
}
