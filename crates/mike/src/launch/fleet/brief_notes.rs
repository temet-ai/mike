use uuid::Uuid;

use super::definition::{FleetDefinition, TaskDefinition};
use super::plan::{FleetFacts, criteria_block, facts_block_reading, list};
use super::plan_briefs::{WORKER_PURPOSE, check_line, read_the_fleet, reference_line};

pub const BRIEF_KIND: &str = "brief";
pub const PART_RUN: &str = "part:run";
pub const PART_TASK: &str = "part:task";

pub fn as_notes(def: &FleetDefinition) -> bool {
    def.briefs.as_notes
}

pub fn fleet_tag(fleet: &str) -> String {
    format!("fleet:{fleet}")
}

pub fn task_tag(task: &str) -> String {
    format!("task:{task}")
}

pub fn run_tags(fleet: &str) -> Vec<String> {
    vec![fleet_tag(fleet), PART_RUN.to_string()]
}

pub fn task_tags(fleet: &str, task: &str) -> Vec<String> {
    vec![fleet_tag(fleet), task_tag(task), PART_TASK.to_string()]
}

pub fn run_part(def: &FleetDefinition, facts: &FleetFacts) -> String {
    format!(
        "The run's part of every brief, fleet {slug}. Every seat of the run works under it; each seat's own task is in its own brief.\n\n{facts}\n\nThe operator's ask, verbatim:\n{ask}\n\nAcceptance criteria, in the operator's words:\n{criteria}\n\nRules particular to this run:\n{rules}\n\nHow this project is built and checked: {check}\n\nFiles shared with other tasks; agree by mail with the other task's seat before you change one:\n{shared}\n\n{working}",
        slug = facts.slug,
        facts = facts_block_reading(def, facts, &read_the_fleet(facts)),
        ask = def.ask.trim(),
        criteria = criteria_block(def, None),
        rules = list(&def.rules),
        check = build_and_check(def, facts),
        shared = list(&def.shared_files),
        working = working_lines(def, facts),
    )
}

fn build_and_check(def: &FleetDefinition, facts: &FleetFacts) -> String {
    let project = facts
        .paths
        .project
        .as_deref()
        .map(|path| format!("the project's own file is {path}. "))
        .unwrap_or_default();
    format!(
        "{project}Before a check or test, read list_fleet_results: still_valid null is unproven, false invalidated; inspect receipts. {}",
        check_line(def, &facts.fleet).trim_end()
    )
}

fn working_lines(def: &FleetDefinition, facts: &FleetFacts) -> String {
    format!(
        "How a worker reports and works:\nTo find where something is defined: fleet_where (fleet {fleet}, symbol or path). {reference}What you change is read from the tree by mike; commit with fleet_commit (fleet {fleet}, your session id, a message), never with git add or git commit. Write a finding note when you learn something another seat will need.\nWhat changed in your paths since you last looked comes to you as lines at the start of a turn and on the answers of fleet_commit, fleet_check and notes; what_changed view mine lists your own uncommitted paths and whether the check ran after your last change.\nNeed something from another seat, or stuck for ten minutes: ask (to a session id, task:<id>, owner:<path> or reference; blocked true when you cannot go on) and carry on; answer or decline the questions standing lists for you before your next slice. A question no seat can answer goes by ask with blocked true; mike's problem-solver decides it. A task that does not exist yet is added with fleet_propose, and part of yours moved to a new task with fleet_split. There is no Help: mail.\nMail a seat only for what only a session knows: to a session id, task:<id>, owner:<path> or fleet. A send to an ended seat is refused naming who has its task now; send again to that seat. Mail is acknowledged by your next tool call after it arrives: never call ack_session_message, except to decline a message you will not act on.\nReport each finished slice with fleet_report (fleet {fleet}, your session id, your task id, the slice, the criteria, what to look at, the paths of your evidence), never by mail: mike adds the commit, the check verdict at that commit, the brief version you read and whether the check ran after your last change, runs the task's floor at that commit and has the judge read the report; a failure comes to you as mike's message with the reason. Then start the next slice.\nTo run or time a built program while the tree changes, build it once in this checkout and copy the program out; never create a second worktree or checkout for it, whatever the project's own instruction files recommend.\nFormat only your own files, by path; never run a command that formats a whole package, crate or project.\nEnd your bounded session after handing off your one task with runnable scenario, environment and observable effects. mike checks each pushed commit and tells the seat that broke it; do not wait for a gate before you commit or report.",
        fleet = facts.fleet,
        reference = reference_line(def),
    )
}

pub fn task_part(def: &FleetDefinition, task_id: &str) -> Option<String> {
    let task = def.tasks.get(task_id)?;
    Some(task_part_of(task_id, task))
}

fn task_part_of(task_id: &str, task: &TaskDefinition) -> String {
    format!(
        "Your task is {task_id}: {name}.\n\n{brief}\n\nCriteria this task serves: {criteria}\n\nFiles you may touch:\n{files}\n\nFiles this task creates:\n{new_files}",
        name = task.name,
        brief = task.brief.trim(),
        criteria = if task.criteria.is_empty() {
            "none".to_string()
        } else {
            task.criteria.join(", ")
        },
        files = list(&task.files),
        new_files = list(&task.new_files),
    )
}

pub fn worker_file(
    def: &FleetDefinition,
    facts: &FleetFacts,
    (seat, task_id): (&str, &str),
    run_note: Option<Uuid>,
) -> Option<String> {
    let task = task_part(def, task_id)?;
    Some(format!(
        "{WORKER_PURPOSE}\n\nYou are seat {seat} of fleet {slug}, a worker.\n\n{task}\n\n{pointer}{goal}",
        slug = facts.slug,
        pointer = run_pointer(def, facts, run_note),
        goal = super::goals::brief_line(def, seat),
    ))
}

fn run_pointer(def: &FleetDefinition, facts: &FleetFacts, run_note: Option<Uuid>) -> String {
    let named = match run_note {
        Some(note) => format!("mike session note {note}"),
        None => "the mike session note tagged part:run".to_string(),
    };
    format!(
        "The run's part of every brief (the goal, the operator's ask, the criteria, the rules, how the project is built and checked, the shared files and how to report) is {named}, kind \"{BRIEF_KIND}\", tags {fleet} and {PART_RUN}, in the coordinator's notebook. It is already in your context from the template you were forked from; if you do not find it above, read it with list_session_notes (session_id={coordinator}, kind=\"{BRIEF_KIND}\", tag=\"{PART_RUN}\"). The other seats are not listed here: read them with get_fleet (fleet {fleet_id}, view \"seat\", your own session id). The one checkout is {repo}.",
        fleet = fleet_tag(&facts.fleet),
        coordinator = facts.seats.coordinator,
        fleet_id = facts.fleet,
        repo = def.repo,
    )
}

pub const AMENDMENTS_MARK: &str = "Answers that change this brief:";

fn amendments_at(body: &str) -> Option<usize> {
    body.find(&format!("\n\n{AMENDMENTS_MARK}"))
}

pub fn amended(body: &str, entry: &str) -> String {
    let body = body.trim_end();
    if amendments_at(body).is_some() {
        format!("{body}\n\n{}", entry.trim())
    } else {
        format!("{body}\n\n{AMENDMENTS_MARK}\n{}", entry.trim())
    }
}

pub fn carrying_amendments(fresh: &str, previous: &str) -> String {
    match amendments_at(previous) {
        Some(at) => format!("{}{}", fresh.trim_end(), previous[at..].trim_end()),
        None => fresh.to_string(),
    }
}

pub fn lines_that_differ(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let removed = old_lines
        .iter()
        .filter(|line| !line.trim().is_empty() && !new_lines.contains(line))
        .map(|line| format!("- {line}"));
    let added = new_lines
        .iter()
        .filter(|line| !line.trim().is_empty() && !old_lines.contains(line))
        .map(|line| format!("+ {line}"));
    removed.chain(added).collect::<Vec<_>>().join("\n")
}

pub fn change_mail(what: &str, (old, new): (i64, i64), differing: &str, note: Uuid) -> String {
    format!(
        "mike: {what} changed (version {old}, now {new}). Only the lines that differ follow; \"-\" is what was removed and \"+\" is what is there now. The whole part is mike session note {note}.\n\n{differing}"
    )
}
