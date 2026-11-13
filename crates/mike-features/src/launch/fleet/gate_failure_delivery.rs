use super::*;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;

#[path = "gate_failure_handoff.rs"]
pub mod handoff;

const NOTICE_COMMAND_BYTES: usize = 1024;
const NOTICE_MAX_ENTRIES: usize = 50;
const NOTICE_MAX_BYTES: usize = 8 * 1024;
const NOTICE_OMISSION_RESERVE: usize = 160;

pub async fn tell_owners(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    target: &Target,
    (code, output, log): (Option<i32>, &str, &Path),
) -> BTreeSet<Uuid> {
    let current = current_definition(persist, def, run).await;
    let files = tracked_files(def, target).await;
    let mut routes = routes(
        &current,
        target,
        files.as_deref().unwrap_or_default(),
        output,
    );
    if files.is_none() {
        routes.unresolved.insert(
            "Committed file listing unavailable; file ownership could not be resolved.".into(),
        );
    }
    handoff::add(persist, run, &mut routes).await;
    let recipients = recipients(persist, &mut routes).await;
    let heading = heading(&def.gate.command, &target.commit, code, log);
    let mut sent = BTreeSet::new();
    for (session, failures) in recipients {
        let body = format!("{heading}\nFile ownership contacts:\n{}", listed(&failures));
        match send(persist, session, &body).await {
            Ok(()) => {
                sent.insert(session);
            }
            Err(problem) => {
                unavailable(&mut routes.unresolved, &problem, &failures);
            }
        }
    }
    tell_unresolved(persist, &current, &heading, &routes.unresolved).await;
    sent
}

fn heading(command: &str, commit: &str, code: Option<i32>, log: &Path) -> String {
    let command = notice_command(command);
    let verdict = code.map_or_else(
        || "did not complete (timeout or no exit code)".to_owned(),
        |code| format!("failed (exit {code})"),
    );
    format!(
        "[mike gate] `{command}` at commit {commit} {verdict}. Observed test failures are routed by declared file ownership, which does not establish who introduced a failure. Investigate the listed files and coordinate repairs. Unparsed or unrun tests may be missing. Full captured output: {} (also retained in the persisted gate result for this command and commit).",
        log.display()
    )
}

fn notice_command(command: &str) -> String {
    if command.len() <= NOTICE_COMMAND_BYTES {
        return command.to_owned();
    }
    let mut end = NOTICE_COMMAND_BYTES;
    while !command.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{} [command truncated: {} UTF-8 bytes omitted; exact command in the persisted gate result]",
        &command[..end],
        command.len() - end
    )
}

fn listed(failures: &BTreeSet<String>) -> String {
    let mut body = String::new();
    let mut shown = 0;
    for failure in failures.iter().take(NOTICE_MAX_ENTRIES) {
        if body.len() + failure.len() + 3 > NOTICE_MAX_BYTES - NOTICE_OMISSION_RESERVE {
            break;
        }
        if shown > 0 {
            body.push('\n');
        }
        body.push_str("- ");
        body.push_str(failure);
        shown += 1;
    }
    let omitted = failures.len() - shown;
    if omitted > 0 {
        body.push_str(&format!("\n{omitted} additional detail entries omitted; full evidence is in the captured gate log or persisted result referenced above."));
    }
    body
}

fn unavailable(unresolved: &mut BTreeSet<String>, problem: &str, failures: &BTreeSet<String>) {
    for failure in failures {
        unresolved.insert(format!("{problem}; {failure}"));
    }
}

async fn current_definition(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
) -> FleetDefinition {
    let session = def
        .coordinator
        .session
        .iter()
        .chain(def.acceptance.session.iter())
        .chain(def.tasks.values().filter_map(|task| task.session.as_ref()))
        .find_map(|s| s.parse::<Uuid>().ok());
    let Some(session) = session else {
        return def.clone();
    };
    persist
        .fleet()
        .find_fleet_for_session(session)
        .await
        .ok()
        .flatten()
        .filter(|row| row.run_id.as_deref() == Some(run))
        .and_then(|row| FleetDefinition::from_value(&row.definition).ok())
        .unwrap_or_else(|| def.clone())
}

async fn recipient(persist: &PersistHandle, session: Uuid) -> Result<(Uuid, bool), String> {
    let closed = persist
        .launch()
        .closed_fleet_seat(session)
        .await
        .map_err(|error| format!("Could not check recipient {session}: {error}"))?;
    match closed {
        None => Ok((session, false)),
        Some(closed) => closed
            .successor
            .or(closed.coordinator)
            .map(|next| (next, true))
            .ok_or_else(|| {
                format!("Owner session {session} is closed with no open successor or coordinator")
            }),
    }
}

async fn recipients(
    persist: &PersistHandle,
    routes: &mut Routes,
) -> BTreeMap<Uuid, BTreeSet<String>> {
    let mut recipients = BTreeMap::<Uuid, BTreeSet<String>>::new();
    for (session, failures) in &routes.owners {
        match recipient(persist, *session).await {
            Ok((next, redirected)) => {
                let entry = recipients.entry(next).or_default();
                entry.extend(failures.iter().cloned());
                if redirected {
                    entry.insert(format!("Owner session {session} is closed; routed to its open successor or coordinator {next}."));
                }
            }
            Err(problem) => {
                unavailable(&mut routes.unresolved, &problem, failures);
            }
        }
    }
    recipients
}

async fn send(persist: &PersistHandle, session: Uuid, body: &str) -> Result<(), String> {
    match persist
        .launch()
        .insert_message(Uuid::new_v4(), session, None, body)
        .await
    {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(format!(
            "Gate owner notice was not queued: recipient {session} is missing or closed"
        )),
        Err(error) => Err(format!(
            "Gate owner notice for {session} could not be queued: {error}"
        )),
    }
}

async fn tell_unresolved(
    persist: &PersistHandle,
    def: &FleetDefinition,
    heading: &str,
    unresolved: &BTreeSet<String>,
) {
    if unresolved.is_empty() {
        return;
    }
    let Some(session) = def
        .coordinator
        .session
        .as_deref()
        .and_then(|s| s.parse().ok())
    else {
        warn_no_coordinator(unresolved);
        return;
    };
    let body = format!(
        "{heading}\nUnresolved or ambiguous ownership and delivery:\n{}",
        listed(unresolved)
    );
    if let Err(problem) = send_current(persist, session, &body).await {
        warn_unresolved_delivery(&problem, unresolved);
    }
}

async fn send_current(persist: &PersistHandle, session: Uuid, body: &str) -> Result<(), String> {
    let (current, _) = recipient(persist, session).await?;
    send(persist, current, body).await
}

fn warn_no_coordinator(unresolved: &BTreeSet<String>) {
    tracing::warn!(details = %listed(unresolved), "fleet: gate ownership unresolved; no coordinator session");
}

fn warn_unresolved_delivery(problem: &str, unresolved: &BTreeSet<String>) {
    tracing::warn!(%problem, details = %listed(unresolved), "fleet: could not mail unresolved gate ownership; full gate output remains persisted");
}

#[cfg(test)]
#[path = "gate_failure_delivery_tests.rs"]
pub mod tests;
