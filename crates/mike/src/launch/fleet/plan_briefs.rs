use super::definition::{FleetDefinition, TaskDefinition};
use super::plan::{FleetFacts, criteria_block, facts_block_reading, list, ordered_tasks};

pub(super) const WORKER_PURPOSE: &str =
    "Your core purpose: finish your one task, fully, and help the other seats finish theirs.";

pub(super) fn worker_prompt(
    def: &FleetDefinition,
    facts: &FleetFacts,
    (seat, task_id): (&str, &str),
    task: &TaskDefinition,
) -> String {
    format!(
        "{WORKER_PURPOSE}\n\nYou are seat {seat} of fleet {slug}, a worker.\n\nYour task is {task_id}: {name}.\n\n{brief}\n{commit_waits}\n{path_contracts}\n\n{facts}\n\nCriteria this task serves:\n{criteria}\n\nFiles you may touch:\n{files}\n\nFiles shared with other tasks; follow the named producer above. For historical sharing without a named producer, agree with the other task's seat before changing a file:\n{shared}\n\nThe other tasks' seats as they were when you started, by name, session and task; mail a seat by its session. fleet_where names the owner of a file. A seat that starts later is in get_fleet, and in `mike fleet status` in your pane:\n{seats}\n\nRules particular to this run:\n{rules}\n\nBefore your first slice, read the run's notes (list_session_notes) for what each of your criteria needs to see.\nBefore a check or test, read list_fleet_results: still_valid null is unproven, false invalidated; inspect receipts. {check}\nTo find where something is defined: fleet_where (fleet {fleet}, symbol or path). {reference}What you change is read from the tree by mike; commit with fleet_commit (fleet {fleet}, your session id, a message), never with git add or git commit. Write a finding note when you learn something another seat will need.\nWhat changed in your paths since you last looked comes to you as lines at the start of a turn and on the answers of fleet_commit and notes; what_changed view mine lists your own uncommitted paths and whether the check ran after your last change.\nNeed something from another seat, or stuck for ten minutes: ask (to a session id, task:<id>, owner:<path> or reference; blocked true when you cannot go on) and carry on; answer or decline the questions standing lists for you before your next slice. A question no seat can answer goes by ask with blocked true; mike's problem-solver decides it. A task that does not exist yet is added with fleet_propose, and part of yours moved to a new task with fleet_split. There is no Help: mail.\nMail a seat only for what only a session knows: to a session id, task:<id>, owner:<path> or fleet. A send to an ended seat is refused naming who has its task now; send again to that seat. Mail is acknowledged by your next tool call after it arrives: never call ack_session_message, except to decline a message you will not act on.\nReport finished slices with fleet_report (example below), never mail. mike adds commit, check verdict/freshness and brief version, runs the task's floor at that commit and has the judge read the report; a failure comes to you as mike's message with the reason. Then start the next slice.\nTo run or time a built program while the tree changes, build it once in this checkout and copy the program out; never create a second worktree or checkout for it, whatever the project's own instruction files recommend.\nFormat only your own files, by path; never run a command that formats a whole package, crate or project.\nEnd your bounded session after handing off your one task with runnable scenario, environment and observable effects. A passing floor and judge release dependent work. mike checks each pushed commit and routes named failures to the owning seat; do not wait for a gate before you commit or report. Distinct follow-up work gets its own task and fresh session.{goal}",
        goal = super::goals::brief_line(def, seat),
        slug = facts.slug,
        name = task.name,
        brief = task.brief.trim(),
        commit_waits = super::tell_commits::brief(task),
        path_contracts = super::checks_paths::brief(def, task),
        facts = facts_block_reading(def, facts, &read_the_fleet(facts)),
        criteria = criteria_block(def, Some(&task.criteria)),
        files = list(&task.files),
        shared = list(&def.shared_files),
        seats = other_seats(def, task_id),
        rules = list(&def.rules),
        fleet = facts.fleet,
        check = check_line(def, &facts.fleet),
        reference = reference_line(def),
    )
}

fn other_seats(def: &FleetDefinition, own_task: &str) -> String {
    let seated: Vec<String> = ordered_tasks(def)
        .into_iter()
        .filter(|(id, _)| id.as_str() != own_task)
        .filter_map(|(id, task)| {
            let (seat, session) = (task.seat.as_deref()?, task.session.as_deref()?);
            Some(format!("{id} {}: {seat}, session {session}", task.name))
        })
        .collect();
    if seated.is_empty() {
        return "- no other task has a seat yet".to_string();
    }
    list(&seated)
}

pub(super) fn check_line(def: &FleetDefinition, fleet: &str) -> String {
    format!(
        "{}\n{}",
        check_instructions(def, fleet),
        super::operation::context(def)
    )
}

fn check_instructions(def: &FleetDefinition, fleet: &str) -> String {
    if !def.check.declared() {
        return format!(
            "Queue project builds/tests with fleet_check (fleet {fleet}, your session id, command); continue work until completion mail. "
        );
    }
    format!(
        "mike runs `{}` automatically for new fleet commits in its isolated checkout and routes failures to owners. Queue narrow pre-commit checks with fleet_check(command); queued/running receipts are not passes. Continue work; completion arrives by mail. After committing, ask fleet_check (fleet {fleet}, your session id) for the newest verdict, grouped by ownership. ",
        def.check.command.trim()
    )
}

pub(super) fn reference_line(def: &FleetDefinition) -> String {
    match def.reference.session.as_deref() {
        Some(session) if def.reference.enabled => format!(
            "Ask the reference seat first: a question about where something is, how something works, or whether anyone has hit a problem goes to the reference seat, session {session}, with ask (to: reference), and it answers with file and line, before you search the tree yourself. mike mails you a one-line reminder when you read outside your task's files, when a slice fails and when you are ten minutes in without a commit. "
        ),
        _ => String::new(),
    }
}

pub(super) fn read_the_fleet(facts: &FleetFacts) -> String {
    format!(
        "Read the fleet: get_fleet fleet={} view \"seat\" and your own session id returns your task and your peers, without briefs",
        facts.fleet
    )
}
