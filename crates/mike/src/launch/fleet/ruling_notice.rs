use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::status::AcceptanceObligation;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::SessionNoteRow;

pub(super) fn list(entries: &[AcceptanceObligation], task: &str) -> String {
    let rules = super::ruling::current_rules(entries, task);
    if rules.is_empty() {
        return String::new();
    }
    let mut lines = vec![format!(
        "[mike executable rulings] Task {task}: this is the current acceptance-owned list. Run each required ruling with fleet_check(ruling=<note UUID>, commit=<your reported SHA>); fleet_report reads exact clean-commit receipts. An amended brief does not replace these rulings."
    )];
    for entry in rules {
        match super::ruling::executable(entry) {
            Ok(rule) => lines.push(format!(
                "- {}: note {}, version {}, {}, expected exit {}. Command: {}",
                entry.obligation_id,
                entry.note_id,
                rule.version,
                if entry.required {
                    "required"
                } else {
                    "optional"
                },
                rule.expected_exit_code,
                rule.command
            )),
            Err(problem) => lines.push(format!("- {}: {problem}", entry.obligation_id)),
        }
    }
    lines.join("\n")
}

pub async fn prompt(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    task: &str,
) -> String {
    let Some(acceptance) = super::report::acceptance_session(def) else {
        return String::new();
    };
    let fleet = match persist
        .fleet()
        .find_fleet_header_for_session(acceptance)
        .await
    {
        Ok(Some(fleet)) if fleet.run_id.as_deref() == Some(run) => fleet,
        Ok(_) => return String::new(),
        Err(err) => return format!("Executable ruling list is unavailable: {err}"),
    };
    match super::ruling::registered(persist, def, &fleet.fleet_uuid.to_string(), run).await {
        Ok(entries) => list(&entries, task),
        Err(problem) => problem,
    }
}

pub async fn after_note(persist: &PersistHandle, note: &SessionNoteRow) {
    if note.data.get("ruling").is_none()
        || !note.tags.iter().any(|tag| tag == "acceptance-obligation")
    {
        return;
    }
    if let Err(problem) = notify(persist, note).await {
        tracing::warn!(note = %note.note_uuid, %problem, "fleet ruling: current list could not be mailed");
    }
}

async fn notify(persist: &PersistHandle, note: &SessionNoteRow) -> Result<(), String> {
    let fleet = note
        .tags
        .iter()
        .find_map(|tag| tag.strip_prefix("fleet:"))
        .ok_or("ruling has no fleet")?;
    let row = persist
        .fleet()
        .find_fleet(fleet)
        .await
        .map_err(|err| err.to_string())?
        .ok_or("ruling fleet disappeared")?;
    let def = FleetDefinition::from_value(&row.definition)?;
    let run = row.run_id.ok_or("ruling fleet has no run")?;
    let entries = super::ruling::registered(persist, &def, fleet, &run).await?;
    for task in super::acceptance::tasks_named(&note.data) {
        let Some(session) = def.tasks.get(&task).and_then(super::report::task_session) else {
            continue;
        };
        let closed = persist
            .launch()
            .closed_fleet_seat(session)
            .await
            .map_err(|err| err.to_string())?;
        let target = closed.map_or(Some(session), |closed| closed.successor);
        if let Some(target) = target {
            persist
                .launch()
                .insert_message(Uuid::new_v4(), target, None, &list(&entries, &task))
                .await
                .map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}
