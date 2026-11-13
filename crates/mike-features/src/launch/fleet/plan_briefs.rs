use super::definition::{FleetDefinition, TaskDefinition};
use super::plan::{FleetFacts, criteria_block, facts_block_reading, list, ordered_tasks};

pub const WORKER_PURPOSE: &str =
    "Your core purpose: finish your one task, fully, and help the other seats finish theirs.";

pub fn worker_prompt(
    def: &FleetDefinition,
    facts: &FleetFacts,
    (seat, task_id): (&str, &str),
    task: &TaskDefinition,
) -> String {
    format!(
        "{WORKER_PURPOSE}\n\nYou are seat {seat} of fleet {slug}, a worker.\n\nYour task is {task_id}: {name}.\n\n{brief}\n{commit_waits}\n{path_contracts}\n\n{facts}\n\nCriteria this task serves:\n{criteria}\n\nFiles you may touch:\n{files}\n\nFiles shared with other tasks; follow the named producer above. For historical sharing without a named producer, agree with the other task's seat before changing a file:\n{shared}\n\nThe other tasks' seats as they were when you started, by name, session and task; mail a seat by its session. fleet_where names the owner of a file. A seat that starts later is in get_fleet, and in `mike fleet status` in your pane:\n{seats}\n\nRules particular to this run:\n{rules}\n\n{working}{goal}",
        working = working_facts(def, &facts.fleet),
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
    )
}

pub fn working_facts(def: &FleetDefinition, fleet: &str) -> String {
    format!(
        "{check}\nlist_fleet_results lists earlier command results in this checkout: still_valid null is unproven, false invalidated.\nTo find where something is defined: fleet_where (fleet {fleet}, symbol or path). {reference}Commit with fleet_commit (fleet {fleet}, your session id, a message): mike reads what you change from the tree. The checkout's index is shared, so git add and git commit would take other seats' files.\nWhat changed in your paths since you last looked arrives as lines at the start of a turn and on the answers of fleet_commit and notes; what_changed view mine lists your own uncommitted paths.\nTo reach another seat: ask (to a session id, task:<id>, owner:<path> or reference; blocked true when you cannot go on), or mail (to a session id, task:<id>, owner:<path> or fleet). A send to an ended seat is refused naming who has its task now. A question no seat can answer goes by ask with blocked true; mike's problem-solver decides it. Mail is acknowledged by your next tool call after it arrives; call ack_session_message only to decline a message. fleet_propose adds a task; fleet_split moves part of yours into a new one.\nReport with fleet_report: mike adds the commit and the check verdict at it, runs the task's floor at that commit and has the judge read the report; a failure comes back as mike's message with the reason. A passing floor and judge release dependent work.\nmike checks each pushed commit and tells the seat that broke it.",
        check = check_line(def, fleet),
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

pub fn check_line(def: &FleetDefinition, fleet: &str) -> String {
    format!(
        "{}\n{}",
        check_instructions(def, fleet),
        super::operation::context(def)
    )
}

fn check_instructions(def: &FleetDefinition, fleet: &str) -> String {
    if !def.check.declared() {
        return format!(
            "fleet_check (fleet {fleet}, your session id, command) runs a project build or test; its result arrives by mail. "
        );
    }
    format!(
        "mike runs `{}` for new fleet commits in its isolated checkout and sends failures to their owners. fleet_check (fleet {fleet}, your session id) runs the declared check, a test or a command you pass; its result arrives by mail. ",
        def.check.command.trim()
    )
}

pub fn reference_line(def: &FleetDefinition) -> String {
    match def.reference.session.as_deref() {
        Some(session) if def.reference.enabled => format!(
            "The reference seat, session {session}, answers where something is, how something works and whether anyone has hit a problem, with file and line: ask (to: reference). "
        ),
        _ => String::new(),
    }
}

pub fn read_the_fleet(facts: &FleetFacts) -> String {
    format!(
        "Read the fleet: get_fleet fleet={} view \"seat\" and your own session id returns your task and your peers, without briefs",
        facts.fleet
    )
}
