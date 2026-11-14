use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::bundle::FleetPaths;
use super::definition::{FleetDefinition, SeatDefinition, TaskDefinition, natural_order};

pub use crate::launch::seat::TEMPLATE_SEAT_PREFIX;

const REFERENCE_PURPOSE: &str = "Your core purpose: answer the other seats' questions about the system with evidence, so no seat searches for what the run already knows.";
pub const REFERENCE_SEAT: &str = "reference";
const ONE_CHECKOUT: &str = "Nobody adds a worktree, a second checkout or a branch. What this project's own instruction files say about worktrees does not apply in a fleet.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatPlan {
    #[serde(default)]
    pub runtime: crate::runtime::RuntimeKind,
    pub seat: String,
    pub role: String,
    pub discipline: Option<String>,
    pub model: String,
    #[serde(default)]
    pub effort: Option<String>,
    pub agent: String,
    pub contract: Option<String>,
    pub task: Option<String>,
    pub session: Uuid,
    pub goal: String,
    pub instructions: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlSeats {
    pub coordinator: Option<Uuid>,
    pub acceptance: Option<Uuid>,
    pub observer: Option<Uuid>,
}

impl ControlSeats {
    pub fn recorded(def: &FleetDefinition) -> Self {
        Self {
            coordinator: session_of(&def.coordinator),
            acceptance: session_of(&def.acceptance),
            observer: None,
        }
    }

    pub fn answering(coordinator: Option<Uuid>, acceptance: Option<Uuid>) -> Self {
        Self {
            coordinator,
            acceptance,
            observer: None,
        }
    }
}

fn session_of(seat: &SeatDefinition) -> Option<Uuid> {
    seat.session.as_deref()?.trim().parse().ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetFacts {
    pub fleet: String,
    pub slug: String,
    pub run_id: String,
    pub seats: ControlSeats,
    pub paths: FleetPaths,
}

pub fn ordered_tasks(def: &FleetDefinition) -> Vec<(&String, &TaskDefinition)> {
    let mut tasks: Vec<(&String, &TaskDefinition)> = def.tasks.iter().collect();
    tasks.sort_by_key(|(id, _)| natural_order(id));
    tasks
}

pub fn ready_tasks(def: &FleetDefinition, passed: &BTreeSet<String>) -> Vec<String> {
    ordered_tasks(def)
        .into_iter()
        .filter(|(_, task)| task.after.iter().all(|before| passed.contains(before)))
        .map(|(id, _)| id.clone())
        .collect()
}

pub fn waiting_tasks(def: &FleetDefinition) -> Vec<&String> {
    ordered_tasks(def)
        .into_iter()
        .filter(|(_, task)| !task.after.is_empty())
        .map(|(id, _)| id)
        .collect()
}

pub fn warm_disciplines(def: &FleetDefinition) -> Vec<String> {
    def.workers
        .keys()
        .filter(|discipline| {
            def.tasks
                .values()
                .any(|task| task.discipline == **discipline)
        })
        .cloned()
        .collect()
}

pub fn template_name(run_id: &str, discipline: &str) -> String {
    format!("{run_id}:{discipline}")
}

pub fn worker_seat_name(discipline: &str, task: &str) -> String {
    format!("{discipline}-{task}")
}

pub fn template(def: &FleetDefinition, facts: &FleetFacts, discipline: &str) -> Option<SeatPlan> {
    let settings = def.workers.get(discipline)?;
    Some(SeatPlan {
        seat: format!("{TEMPLATE_SEAT_PREFIX}{discipline}"),
        role: format!("{TEMPLATE_SEAT_PREFIX}{discipline}"),
        discipline: Some(discipline.to_string()),
        runtime: settings.runtime,
        model: settings.model.clone(),
        effort: settings.effort.clone(),
        agent: settings.agent.clone(),
        contract: settings.contract_file(),
        task: None,
        session: Uuid::new_v4(),
        goal: format!("{discipline} template of fleet {}", facts.slug),
        instructions: format!(
            "{discipline} template of fleet {}, run {}. Loads context and stops; the run's {discipline} workers are forked from it.",
            facts.slug, facts.run_id
        ),
        prompt: template_prompt(def, facts, discipline, &settings.agent),
    })
}

pub fn reference(
    def: &FleetDefinition,
    facts: &FleetFacts,
    discipline: &str,
    session: Uuid,
) -> Option<SeatPlan> {
    if !def.reference.enabled {
        return None;
    }
    let settings = def.workers.get(discipline)?;
    Some(SeatPlan {
        seat: REFERENCE_SEAT.to_string(),
        role: REFERENCE_SEAT.to_string(),
        discipline: Some(discipline.to_string()),
        runtime: settings.runtime,
        model: settings.model.clone(),
        effort: settings.effort.clone(),
        agent: settings.agent.clone(),
        contract: settings.contract_file(),
        task: None,
        session,
        goal: format!(
            "reference of fleet {}: answers where and how questions",
            facts.slug
        ),
        instructions: format!(
            "reference seat of fleet {}, run {}. Forked from the {discipline} template; answers the other seats' questions about the system from a read it just made, with file and line. Never edits, builds, assigns work or decides anything.",
            facts.slug, facts.run_id
        ),
        prompt: reference_prompt(def, facts, session),
    })
}

fn reference_prompt(def: &FleetDefinition, facts: &FleetFacts, session: Uuid) -> String {
    format!(
        "{REFERENCE_PURPOSE}\n\nYou are the reference seat of fleet {slug}. You were forked from the run's template, so you hold what every worker started with: the rules, the architecture and the definition.\n\n{facts}\n\nHow you work:\n- Wait for questions. Each one comes from a seat: where something is, how something works, whether anyone has hit a problem, what changed in an area.\n- Before answering, read what changed since you last looked: list_fleet_changes (fleet {fleet}) for the paths the question touches, then read those files. Use fleet_where (fleet {fleet}, symbol or path) to find a definition.\n- Answer each question with answer (or decline when you cannot), with the file and line of a read you just made; the question's record is what the asker reads. Mail the asker only to add what the record cannot hold. Say when you did not find it; never answer from memory alone.\n- Read the run's notes (list_session_notes) so 'has anyone hit this' has one answer.\n- Mail is acknowledged by your next tool call; call ack_session_message only to decline a message you will not answer.\n- You never edit a file, build, test, assign work or decide anything; those stay with the workers and mike's judge and problem-solver.\n\n{examples}\n\nEnd this first turn with the single line READY and wait for mail.",
        slug = facts.slug,
        facts = facts_block(def, facts),
        fleet = facts.fleet,
        examples = super::tool_examples::brief(&facts.fleet, session, None),
    )
}

pub fn worker(
    def: &FleetDefinition,
    facts: &FleetFacts,
    task_id: &str,
    session: Uuid,
) -> Option<SeatPlan> {
    worker_with(def, facts, task_id, session, None)
}

pub fn worker_with(
    def: &FleetDefinition,
    facts: &FleetFacts,
    task_id: &str,
    session: Uuid,
    run_note: Option<Uuid>,
) -> Option<SeatPlan> {
    let task = def.tasks.get(task_id)?;
    let settings = def.workers.get(&task.discipline)?;
    let seat = worker_seat_name(&task.discipline, task_id);
    let prompt = if super::brief_notes::as_notes(def) {
        super::brief_notes::worker_file(def, facts, (&seat, task_id), run_note)?
    } else {
        super::plan_briefs::worker_prompt(def, facts, (&seat, task_id), task)
    };
    Some(SeatPlan {
        prompt: format!(
            "{prompt}\n\n{}",
            super::tool_examples::brief(&facts.fleet, session, Some(task_id))
        ),
        seat,
        role: task.discipline.clone(),
        discipline: Some(task.discipline.clone()),
        runtime: settings.runtime,
        model: settings.model.clone(),
        effort: settings.effort.clone(),
        agent: settings.agent.clone(),
        contract: settings.contract_file(),
        task: Some(task_id.to_string()),
        session,
        goal: format!("{task_id}: {}", task.name),
        instructions: format!(
            "{} worker of fleet {}, run {}. One shared checkout and branch, `{}`. One task; the session ends after its bounded handoff. Readiness and acceptance are separate.",
            task.discipline, facts.slug, facts.run_id, def.branch
        ),
    })
}

pub fn facts_block(def: &FleetDefinition, facts: &FleetFacts) -> String {
    let read = format!(
        "Read the fleet: get_fleet fleet={} select [\"/fleet/state\", \"/fleet/definition/tasks\"] (each task carries its seat and that seat's session)",
        facts.fleet
    );
    facts_block_reading(def, facts, &read)
}

pub fn facts_block_reading(
    def: &FleetDefinition,
    facts: &FleetFacts,
    read_the_fleet: &str,
) -> String {
    format!(
        "Fleet: {slug}\nFleet definition in mike: {fleet}\nRun id: {run}\nGoal: {goal}\nRepository, the one checkout every seat works in: {repo}\nBranch, the one branch every seat commits to: {branch}\n\
         {ONE_CHECKOUT}\n\
         Rules every seat works under: {rules}\n{project}\
         Comms thread: {fleet}. Mail between this fleet's seats is filed there by itself, under each seat's name (seat and session), so the operator and later runs can read the run's conversation. To leave a note for the whole fleet, post with send_session_message thread_id={fleet} and no `to`; a post reaches no one's turn, so mail a seat by its session when it must act.\n\
         {read_the_fleet}\n\
         Read the seats: get_workspace_roster project_path={repo} select \
         [\"/roster/roster/*/name\", \"/roster/roster/*/role\", \"/roster/roster/*/session_id\", \
         \"/roster/roster/*/liveness\", \"/roster/roster/*/session_state\", \"/roster/roster/*/reply_channel_open\"]",
        slug = facts.slug,
        fleet = facts.fleet,
        run = facts.run_id,
        goal = def.goal,
        repo = def.repo,
        branch = def.branch,
        rules = facts.paths.rules,
        project = facts
            .paths
            .project
            .as_deref()
            .map(|p| format!("This project's own rules for building and checking: {p}\n"))
            .unwrap_or_default(),
    )
}

pub fn criteria_block(def: &FleetDefinition, only: Option<&[String]>) -> String {
    let mut ids: Vec<&String> = def.criteria.keys().collect();
    ids.sort_by_key(|id| natural_order(id));
    ids.into_iter()
        .filter(|id| only.is_none_or(|wanted| wanted.contains(id)))
        .filter_map(|id| def.criteria.get(id).map(|c| (id, c)))
        .map(|(id, c)| format!("- {id}: {} (checked by: {})", c.text, c.check))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn list(items: &[String]) -> String {
    if items.is_empty() {
        return "- none".to_string();
    }
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn template_prompt(
    def: &FleetDefinition,
    facts: &FleetFacts,
    discipline: &str,
    agent: &str,
) -> String {
    let shared = if super::brief_notes::as_notes(def) {
        super::brief_notes::run_part(def, facts)
    } else {
        facts_block(def, facts)
    };
    format!(
        "You are the {discipline} template of fleet {slug}. You load context and stop. Every {discipline} worker of this run is forked from your session, so what you read is what each of them starts with.\n\n{shared}\n\nLoad these, in this order, and nothing else:\n1. {rules}\n2. {templates}\n3. The fleet's definition: get_fleet fleet={fleet} select [\"/fleet/definition/criteria\", \"/fleet/definition/tasks\", \"/fleet/definition/shared_files\", \"/fleet/definition/rules\"]\n\nDo not edit, build, test or summarise what you read. End your turn with the single line READY.",
        slug = facts.slug,
        rules = facts.paths.rules,
        templates = template_load_list(&def.repo, discipline, agent),
        fleet = facts.fleet,
    )
}

fn template_load_list(repo: &str, discipline: &str, agent: &str) -> String {
    let named_for = |name: &str| format!("{repo}/.claude/fleet/templates/{name}.md");
    if discipline == agent || agent.trim().is_empty() {
        return format!(
            "{}, if the file exists: follow its load list.",
            named_for(discipline)
        );
    }
    format!(
        "{}, if the file exists: follow its load list. If it does not, {}, if that file exists.",
        named_for(discipline),
        named_for(agent)
    )
}
