use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::definition::{FleetDefinition, TaskDefinition};

const MODELS: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Missing {
    pub item: String,
    pub problem: String,
}

impl Missing {
    pub fn new(item: impl Into<String>, problem: impl Into<String>) -> Self {
        Self {
            item: item.into(),
            problem: problem.into(),
        }
    }
}

pub fn check_definition(def: &FleetDefinition) -> Vec<Missing> {
    let mut found = Vec::new();
    basics(def, &mut found);
    if def.retrospective.enabled {
        model(&def.retrospective.model, "retrospective.model", &mut found);
        found.extend(super::retrospective::validate(def));
    }
    workers(def, &mut found);
    criteria(def, &mut found);
    tasks(def, &mut found);
    goals(def, &mut found);
    if let Err(problem) = super::start_baseline::repair_task(def) {
        found.push(Missing::new("baseline.repair_task", problem));
    }
    found.extend(super::tell_commits::validate(def));
    found.extend(super::checks_paths::validate(def));
    coverage(def, &mut found);
    shared_files(def, &mut found);
    order(def, &mut found);
    found.extend(super::checks_proof::repeated_proof(def));
    found.extend(super::checks_order::late_checks(def));
    found.extend(super::checks_order::long_chain(def));
    found.extend(super::checks_scope::scope(def));
    found.extend(super::operation::validate(def));
    found.extend(super::checks_target::per_task_targets(def));
    found
}

fn require(value: &str, item: &str, found: &mut Vec<Missing>) {
    if value.trim().is_empty() {
        found.push(Missing::new(item, "is empty"));
    }
}

fn model(value: &str, item: &str, found: &mut Vec<Missing>) {
    let name = value.trim();
    if name.is_empty() {
        found.push(Missing::new(item, "is empty"));
    } else if !MODELS.contains(&name) && !name.starts_with("claude-") {
        found.push(Missing::new(
            item,
            format!("`{name}` is not a model; use opus, sonnet, haiku, fable or a full model id"),
        ));
    }
}

fn basics(def: &FleetDefinition, found: &mut Vec<Missing>) {
    require(&def.goal, "goal", found);
    require(&def.ask, "ask", found);
    require(&def.repo, "repo", found);
    require(&def.branch, "branch", found);
    if !def.repo.trim().is_empty() && !def.repo.starts_with('/') {
        found.push(Missing::new("repo", "must be a full path"));
    }
}

const GOAL_MAX_CHARS: usize = 4_000;
const GOAL_PLACEHOLDERS: [&str; 3] = ["task", "name", "criteria"];

fn goals(def: &FleetDefinition, found: &mut Vec<Missing>) {
    for (id, task) in &def.tasks {
        let (item, template) = if task.goal.trim().is_empty() {
            (format!("goals.workers (task {id})"), &def.goals.workers)
        } else {
            (format!("tasks.{id}.goal"), &task.goal)
        };
        let seat = super::plan::worker_seat_name(&task.discipline, id);
        let goal = super::goals::seat_goal(def, &seat);
        if let Some(word) = unknown_placeholder(template) {
            found.push(Missing::new(
                &item,
                format!(
                    "uses {{{word}}}; the placeholders are {{task}}, {{name}} and {{criteria}}"
                ),
            ));
        }
        goal_fits(&item, goal, found);
    }
}

fn goal_fits(item: &str, goal: Option<String>, found: &mut Vec<Missing>) {
    let Some(goal) = goal else {
        return;
    };
    let length = goal.chars().count();
    if length > GOAL_MAX_CHARS {
        found.push(Missing::new(
            item,
            format!(
                "is {length} characters once filled in; Claude Code's /goal takes at most {GOAL_MAX_CHARS}"
            ),
        ));
    }
}

fn unknown_placeholder(text: &str) -> Option<String> {
    text.split('{').skip(1).find_map(|rest| {
        let word = rest.split('}').next()?;
        let is_word =
            !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        (is_word && !GOAL_PLACEHOLDERS.contains(&word)).then(|| word.to_string())
    })
}

const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

fn effort(value: &Option<String>, item: &str, found: &mut Vec<Missing>) {
    let Some(level) = value.as_deref().map(str::trim).filter(|l| !l.is_empty()) else {
        return;
    };
    if !EFFORTS.contains(&level) {
        found.push(Missing::new(
            format!("{item}.effort"),
            format!("`{level}` is not an effort level; use low, medium, high, xhigh or max"),
        ));
    }
}

fn runtime_model(
    worker: &super::definition::WorkerDefinition,
    path: &str,
    found: &mut Vec<Missing>,
) {
    use crate::launch::codex;
    use crate::runtime::RuntimeKind;
    match worker.runtime {
        RuntimeKind::Claude => {
            model(&worker.model, &format!("{path}.model"), found);
            effort(&worker.effort, path, found);
        }
        RuntimeKind::Codex => {
            let problems = [
                ("model", codex::model_problem(&worker.model)),
                ("effort", codex::effort_problem(worker.effort.as_deref())),
                ("agent", codex::agent_problem(&worker.agent)),
            ];
            for (field, problem) in problems {
                if let Some(problem) = problem {
                    found.push(Missing::new(format!("{path}.{field}"), problem));
                }
            }
        }
    }
}

fn workers(def: &FleetDefinition, found: &mut Vec<Missing>) {
    if def.workers.is_empty() {
        found.push(Missing::new("workers", "no discipline has workers"));
    }
    for (discipline, worker) in &def.workers {
        runtime_model(worker, &format!("workers.{discipline}"), found);
    }
}

fn criteria(def: &FleetDefinition, found: &mut Vec<Missing>) {
    if def.criteria.is_empty() {
        found.push(Missing::new("criteria", "there are no acceptance criteria"));
    }
    for (id, criterion) in &def.criteria {
        require(&criterion.text, &format!("criteria.{id}.text"), found);
        require(&criterion.check, &format!("criteria.{id}.check"), found);
    }
}

fn tasks(def: &FleetDefinition, found: &mut Vec<Missing>) {
    if def.tasks.is_empty() {
        found.push(Missing::new("tasks", "there are no tasks"));
    }
    if def.tasks.len() == 1 {
        found.push(Missing::new(
            "tasks",
            "one task is one session, not a fleet: start it with `mike spawn open`, or cut the goal into tasks that can run at the same time",
        ));
    }
    for (id, task) in &def.tasks {
        require(&task.name, &format!("tasks.{id}.name"), found);
        require(&task.brief, &format!("tasks.{id}.brief"), found);
        task_links(def, id, task, found);
        task_wait(id, task, found);
    }
}

pub const MOST_TASKS_WAITED_ON: usize = 3;

fn task_wait(id: &str, task: &TaskDefinition, found: &mut Vec<Missing>) {
    if task.after.len() > MOST_TASKS_WAITED_ON {
        found.push(Missing::new(
            format!("tasks.{id}.after"),
            format!(
                "waits for {} tasks ({}); a task waits on at most {MOST_TASKS_WAITED_ON}. Split it so each part waits only on what its first step needs, and put the end-to-end path, the reference generators and the demo data in the first wave so every seat can run its own slice",
                task.after.len(),
                task.after.join(", ")
            ),
        ));
    }
}

fn task_links(def: &FleetDefinition, id: &str, task: &TaskDefinition, found: &mut Vec<Missing>) {
    if task.discipline.trim().is_empty() {
        found.push(Missing::new(format!("tasks.{id}.discipline"), "is empty"));
    } else if !def.workers.contains_key(&task.discipline) {
        found.push(Missing::new(
            format!("tasks.{id}.discipline"),
            format!("`{}` has no workers", task.discipline),
        ));
    }
    if task.criteria.is_empty() {
        found.push(Missing::new(
            format!("tasks.{id}.criteria"),
            "the task serves no criterion",
        ));
    }
    for criterion in &task.criteria {
        if !def.criteria.contains_key(criterion) {
            found.push(Missing::new(
                format!("tasks.{id}.criteria"),
                format!("`{criterion}` is not a criterion"),
            ));
        }
    }
    if task.files.is_empty() {
        found.push(Missing::new(
            format!("tasks.{id}.files"),
            "the task names no file it may touch",
        ));
    }
}

fn coverage(def: &FleetDefinition, found: &mut Vec<Missing>) {
    let served: BTreeSet<&str> = def
        .tasks
        .values()
        .flat_map(|t| t.criteria.iter().map(String::as_str))
        .collect();
    for id in def.criteria.keys() {
        if !served.contains(id.as_str()) {
            found.push(Missing::new(
                format!("criteria.{id}"),
                "no task serves this criterion",
            ));
        }
    }
}

pub fn normal(path: &str) -> String {
    let trimmed = path.trim().trim_matches('`');
    let folder = trimmed
        .trim_end_matches("/**")
        .trim_end_matches("/*")
        .trim_end_matches('/');
    folder.to_string()
}

pub fn overlaps(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

fn shared_files(def: &FleetDefinition, found: &mut Vec<Missing>) {
    let shared: BTreeSet<String> = def.shared_files.iter().map(|f| normal(f)).collect();
    let owned: Vec<(&String, String)> = def
        .tasks
        .iter()
        .flat_map(|(id, task)| task.files.iter().map(move |f| (id, normal(f))))
        .filter(|(_, file)| !file.is_empty() && !shared.contains(file))
        .collect();
    let mut reported = BTreeSet::new();
    for (index, (id, file)) in owned.iter().enumerate() {
        for (other, theirs) in &owned[index + 1..] {
            if id != other && overlaps(file, theirs) && reported.insert((*id, *other)) {
                found.push(Missing::new(
                    format!("tasks.{id}.files"),
                    format!("`{file}` is also given to task {other} (`{theirs}`); give it to one task or list it in shared_files"),
                ));
            }
        }
    }
}

fn order(def: &FleetDefinition, found: &mut Vec<Missing>) {
    for (id, task) in &def.tasks {
        for before in &task.after {
            if !def.tasks.contains_key(before) {
                found.push(Missing::new(
                    format!("tasks.{id}.after"),
                    format!("`{before}` is not a task"),
                ));
            }
        }
    }
    let waits: BTreeMap<&str, Vec<&str>> = def
        .tasks
        .iter()
        .map(|(id, t)| (id.as_str(), t.after.iter().map(String::as_str).collect()))
        .collect();
    for id in def.tasks.keys() {
        if waits_on_itself(id, &waits) {
            found.push(Missing::new(
                format!("tasks.{id}.after"),
                "the task ends up waiting on itself",
            ));
        }
    }
}

fn waits_on_itself(start: &str, waits: &BTreeMap<&str, Vec<&str>>) -> bool {
    let mut seen = BTreeSet::new();
    let mut queue: Vec<&str> = waits.get(start).cloned().unwrap_or_default();
    while let Some(next) = queue.pop() {
        if next == start {
            return true;
        }
        if seen.insert(next) {
            queue.extend(waits.get(next).cloned().unwrap_or_default());
        }
    }
    false
}
