use super::definition::{FleetDefinition, TaskDefinition};
use super::plan::{REFERENCE_SEAT, TEMPLATE_SEAT_PREFIX, worker_seat_name};

pub fn seat_goal(def: &FleetDefinition, seat: &str) -> Option<String> {
    match seat {
        REFERENCE_SEAT => None,
        _ if seat.starts_with(TEMPLATE_SEAT_PREFIX) => None,
        _ => worker_goal(def, seat),
    }
}

pub fn brief_line(def: &FleetDefinition, seat: &str) -> String {
    match seat_goal(def, seat) {
        Some(goal) => format!(
            "\n\nYour goal, which the fleet-comms mod arms as Claude Code's /goal after your first turn and a model judges after every turn from what your conversation shows: {goal}"
        ),
        None => String::new(),
    }
}

fn filled(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn worker_goal(def: &FleetDefinition, seat: &str) -> Option<String> {
    let (id, task) = def
        .tasks
        .iter()
        .find(|(id, task)| worker_seat_name(&task.discipline, id) == seat)?;
    let text = filled(&task.goal).or_else(|| filled(&def.goals.workers))?;
    Some(bounded_worker_goal(&filled_in(&text, id, task), id, seat))
}

fn filled_in(text: &str, id: &str, task: &TaskDefinition) -> String {
    text.replace("{task}", id)
        .replace("{name}", &task.name)
        .replace("{criteria}", &task_obligations(id, task))
}

fn task_obligations(id: &str, task: &TaskDefinition) -> String {
    let name = filled(&task.name).unwrap_or_else(|| id.to_string());
    let criteria = task.criteria.join(", ");
    if criteria.is_empty() {
        format!("{name}: task {id}'s assigned contribution per its brief")
    } else {
        format!("{criteria}: only {name}, task {id}'s assigned contribution per its brief")
    }
}

fn bounded_worker_goal(text: &str, id: &str, seat: &str) -> String {
    format!("Task {id}, seat {seat}: {text}")
}

#[cfg(test)]
#[path = "goals_tests.rs"]
pub mod tests;
