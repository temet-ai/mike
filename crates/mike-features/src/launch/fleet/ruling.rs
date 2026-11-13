use crate::store::persistence::fleet_repo::FleetPersist as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::check::{CheckRequest, Runner};
use super::definition::FleetDefinition;
use super::seated::Seated;
use super::status::{AcceptanceObligation, NoteFacts};
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ruling {
    pub version: u64,
    pub command: String,
    pub expected_exit_code: i32,
}

pub fn parse(data: &Value) -> Result<Option<Ruling>, String> {
    let Some(value) = data.get("ruling") else {
        return Ok(None);
    };
    let ruling: Ruling = serde_json::from_value(value.clone())
        .map_err(|err| format!("invalid executable ruling: {err}"))?;
    if ruling.version == 0 || ruling.command.trim().is_empty() || ruling.command.contains('\0') {
        return Err("ruling requires a positive version and a nonempty command without NUL".into());
    }
    if !(0..=125).contains(&ruling.expected_exit_code) {
        return Err("ruling expected_exit_code must be between 0 and 125".into());
    }
    if super::acceptance::tasks_named(data).is_empty() {
        return Err("executable ruling requires explicit affected tasks".into());
    }
    Ok(Some(ruling))
}

pub async fn registered(
    persist: &PersistHandle,
    def: &FleetDefinition,
    fleet: &str,
    run: &str,
) -> Result<Vec<AcceptanceObligation>, String> {
    let notes: Vec<NoteFacts> = persist
        .fleet()
        .fleet_note_rows(run)
        .await
        .map_err(|err| format!("ruling register unavailable: {err}"))?
        .into_iter()
        .map(super::status::note_facts)
        .collect();
    Ok(super::acceptance_register::register(fleet, def, &notes))
}

pub fn executable(entry: &AcceptanceObligation) -> Result<Ruling, String> {
    if let Some(issue) = &entry.issue {
        return Err(format!("ruling {} is invalid: {issue}", entry.note_id));
    }
    parse(&entry.data)?.ok_or_else(|| format!("note {} is not an executable ruling", entry.note_id))
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub fn command(ruling: &Ruling) -> String {
    format!(
        "sh -c {}; mike_ruling_exit=$?; printf '\\n[mike ruling] observed exit: %s; expected: {}\\n' \"$mike_ruling_exit\"; test \"$mike_ruling_exit\" -eq {}",
        quote(&ruling.command),
        ruling.expected_exit_code,
        ruling.expected_exit_code
    )
}

pub fn ledger(run: &str, entry: &AcceptanceObligation, ruling: &Ruling) -> String {
    let identity = serde_json::json!({
        "note": entry.note_id, "ruling": ruling, "tasks": entry.tasks,
        "criteria": entry.criteria, "obligation_id": entry.obligation_id,
    });
    let digest = hex::encode(Sha256::digest(identity.to_string().as_bytes()));
    format!("{run}#ruling:{}:{digest}", entry.note_id)
}

pub fn definition(def: &FleetDefinition, ruling: &Ruling) -> FleetDefinition {
    let mut def = def.clone();
    def.check.command = command(ruling);
    def.check.paths.clear();
    def.check.test_command.clear();
    def
}

pub struct Prepared {
    pub definition: FleetDefinition,
    pub ledger: String,
    pub request: CheckRequest,
}

pub async fn prepare(
    runner: &Runner<'_>,
    def: &FleetDefinition,
    seated: &Seated,
    request: &CheckRequest,
) -> Result<Option<Prepared>, String> {
    let Some(note) = request.ruling else {
        return Ok(None);
    };
    if request.test.is_some() || request.command.is_some() {
        return Err("choose an executable ruling, a command, or a focused test".into());
    }
    if request
        .commit
        .as_deref()
        .is_none_or(|sha| sha.trim().is_empty())
    {
        return Err("an executable ruling requires the reported commit SHA in `commit`".into());
    }
    let entries = registered(runner.persist, def, &runner.fleet.to_string(), runner.run).await?;
    let entry = entries
        .iter()
        .find(|entry| entry.note_id == note)
        .ok_or_else(|| format!("{note} is not a current acceptance ruling of this fleet"))?;
    let ruling = executable(entry)?;
    let who = super::check::asker(def, seated, request.session)
        .ok_or("only a fleet seat can run its registered rulings")?;
    if !who.everything
        && !entry
            .tasks
            .iter()
            .any(|task| seated.holds(request.session, task))
    {
        return Err("this ruling does not apply to the task your seat holds".into());
    }
    let ledger = ledger(runner.run, entry, &ruling);
    super::managed_queue::name_obligation(
        &ledger,
        format!("{}#obligation:{}", runner.run, entry.obligation_id),
    );
    Ok(Some(Prepared {
        definition: definition(def, &ruling),
        ledger,
        request: request.clone(),
    }))
}

pub fn current_rules<'a>(
    entries: &'a [AcceptanceObligation],
    task: &str,
) -> Vec<&'a AcceptanceObligation> {
    entries
        .iter()
        .filter(|entry| {
            entry.data.get("ruling").is_some() && entry.tasks.iter().any(|id| id == task)
        })
        .collect()
}

#[cfg(test)]
#[path = "ruling_tests.rs"]
pub mod tests;
