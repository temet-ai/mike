use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::checks::normal;
use super::definition::{FleetDefinition, TaskDefinition, merge, natural_order};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::questions::AskedQuestion;

pub const HELD: &str = "held";
pub const ADDED: &str = "added";
pub const DECLINED: &str = "declined";
pub const REFUSED: &str = "refused";
pub const BY_MIKE: &str = "mike";
const SAVE_ATTEMPTS: usize = 5;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Proposal {
    pub name: String,
    pub why: String,
    pub files: Vec<String>,
    pub new_files: Vec<String>,
    pub criteria: Vec<String>,
    pub after: Vec<String>,
    pub discipline: String,
}

impl Proposal {
    pub fn all_files(&self) -> Vec<String> {
        self.files
            .iter()
            .chain(&self.new_files)
            .map(|f| normal(f))
            .filter(|f| !f.is_empty())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judged {
    Clean,
    Overlap(BTreeMap<String, Vec<String>>),
    Refused(Vec<String>),
}

fn named(items: impl Iterator<Item = String>) -> String {
    let all: Vec<String> = items.map(|i| format!("`{i}`")).collect();
    if all.is_empty() {
        "none".to_string()
    } else {
        all.join(", ")
    }
}

fn shape_problems(def: &FleetDefinition, p: &Proposal) -> Vec<String> {
    let mut problems = Vec::new();
    if p.name.trim().is_empty() {
        problems.push("`name` is empty: give the task a short name".to_string());
    }
    if p.all_files().is_empty() {
        problems.push("name at least one file in `files` or `new_files`".to_string());
    }
    if p.criteria.is_empty() {
        problems.push(format!(
            "name at least one criterion the task serves in `criteria`; this fleet's criteria are {}",
            named(def.criteria.keys().cloned())
        ));
    }
    problems
}

pub fn problems(def: &FleetDefinition, p: &Proposal, exists: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut found = shape_problems(def, p);
    for file in p.files.iter().map(|f| normal(f)).filter(|f| !f.is_empty()) {
        if !exists(&file) {
            found.push(format!(
                "`{file}` does not exist in the checkout; a file the task will create goes in `new_files`"
            ));
        }
    }
    for criterion in p.criteria.iter().filter(|c| !def.criteria.contains_key(*c)) {
        found.push(format!(
            "`{criterion}` is not a criterion of this fleet; its criteria are {}",
            named(def.criteria.keys().cloned())
        ));
    }
    if !def.workers.contains_key(&p.discipline) {
        found.push(format!(
            "`{}` has no workers in this fleet; `discipline` must be one of {}",
            p.discipline,
            named(def.workers.keys().cloned())
        ));
    }
    for before in p.after.iter().filter(|a| !def.tasks.contains_key(*a)) {
        found.push(format!("`{before}` in `after` is not a task of this fleet"));
    }
    if p.after.iter().all(|id| def.tasks.contains_key(id))
        && !super::checks_paths::valid_dependencies(def, &p.after)
    {
        found.push("`after` reaches a cyclic or missing task dependency; repair that order before adding the task".into());
    }
    if p.after.len() > super::checks::MOST_TASKS_WAITED_ON {
        found.push(format!(
            "`after` names too many tasks; at most {} are allowed",
            super::checks::MOST_TASKS_WAITED_ON
        ));
    }
    found
}

pub fn judge(def: &FleetDefinition, p: &Proposal, exists: &dyn Fn(&str) -> bool) -> Judged {
    judge_done(def, p, exists, &BTreeSet::new())
}

pub fn judge_done(
    def: &FleetDefinition,
    p: &Proposal,
    exists: &dyn Fn(&str) -> bool,
    done: &BTreeSet<String>,
) -> Judged {
    let found = problems(def, p, exists);
    if !found.is_empty() {
        return Judged::Refused(found);
    }
    let owned = super::checks_paths::proposed_owners(
        def,
        (
            &next_task_id(def),
            TaskDefinition {
                files: p.all_files(),
                after: p.after.clone(),
                ..TaskDefinition::default()
            },
        ),
        done,
    );
    if owned.is_empty() {
        Judged::Clean
    } else {
        Judged::Overlap(owned)
    }
}

pub fn next_task_id(def: &FleetDefinition) -> String {
    let mut prefixes: BTreeMap<String, usize> = BTreeMap::new();
    for id in def.tasks.keys() {
        *prefixes.entry(natural_order(id).0).or_default() += 1;
    }
    let prefix = prefixes
        .iter()
        .max_by_key(|(_, count)| **count)
        .map_or_else(|| "T".to_string(), |(p, _)| p.clone());
    let highest = def
        .tasks
        .keys()
        .map(|id| natural_order(id))
        .filter(|(p, _, _)| *p == prefix)
        .map(|(_, n, _)| n)
        .max()
        .unwrap_or(0);
    (highest + 1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !def.tasks.contains_key(id))
        .unwrap_or_else(|| format!("{prefix}{}", highest + 1))
}

pub fn next_proposal_id(def: &FleetDefinition) -> String {
    (def.proposals.len() + 1..)
        .map(|n| format!("proposal-{n}"))
        .find(|id| !def.proposals.contains_key(id))
        .unwrap_or_else(|| format!("proposal-{}", Uuid::new_v4().simple()))
}

fn lines(items: &[String]) -> String {
    if items.is_empty() {
        return "- none".to_string();
    }
    items
        .iter()
        .map(|i| format!("- {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn brief_for(p: &Proposal, by: &str) -> String {
    format!(
        "{name}\n\n{why}\n\nThis task was proposed during the run by {by}.\n\nFiles you may touch:\n{files}\n\nFiles this task creates:\n{new_files}\n\nCriteria it serves: {criteria}.",
        name = p.name.trim(),
        why = p.why.trim(),
        files = lines(&p.files),
        new_files = lines(&p.new_files),
        criteria = p.criteria.join(", "),
    )
}

pub fn task_value(p: &Proposal, by: &str) -> Value {
    json!({
        "name": p.name.trim(),
        "discipline": p.discipline,
        "criteria": p.criteria,
        "files": p.files.iter().chain(&p.new_files).cloned().collect::<Vec<_>>(),
        "new_files": p.new_files,
        "after": p.after,
        "brief": brief_for(p, by),
    })
}

pub struct Record<'a> {
    pub by: &'a str,
    pub state: &'a str,
    pub task: Option<&'a str>,
    pub owners: Vec<String>,
    pub causes: &'a [String],
    pub problems: &'a [String],
}

pub fn proposal_value(p: &Proposal, record: &Record<'_>) -> Value {
    json!({
        "name": p.name, "why": p.why, "files": p.files, "new_files": p.new_files,
        "criteria": p.criteria, "after": p.after, "discipline": p.discipline,
        "by": record.by, "state": record.state, "task": record.task,
        "owners": record.owners, "causes": record.causes, "problems": record.problems,
        "at": Utc::now(),
    })
}

pub fn one(key: &str, id: &str, value: Value) -> Value {
    let mut inner = Map::new();
    inner.insert(id.to_string(), value);
    let mut outer = Map::new();
    outer.insert(key.to_string(), Value::Object(inner));
    Value::Object(outer)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Answer {
    pub proposal: Option<String>,
    pub state: String,
    pub task: Option<String>,
    pub owners: BTreeMap<String, Vec<String>>,
    pub problems: Vec<String>,
}

pub struct Planned {
    pub patch: Option<Value>,
    pub answer: Answer,
}

pub fn plan(
    def: &FleetDefinition,
    p: &Proposal,
    judged: Judged,
    record: (&str, &[String]),
) -> Planned {
    let (by, causes) = record;
    let proposal = next_proposal_id(def);
    let (state, task, owners, problems) = match judged {
        Judged::Clean => (ADDED, Some(next_task_id(def)), BTreeMap::new(), Vec::new()),
        Judged::Overlap(owned) => (HELD, None, owned, Vec::new()),
        Judged::Refused(found) => (REFUSED, None, BTreeMap::new(), found),
    };
    let keep = state != REFUSED || by == BY_MIKE;
    let value = proposal_value(
        p,
        &Record {
            by,
            state,
            task: task.as_deref(),
            owners: owners.keys().cloned().collect(),
            causes,
            problems: &problems,
        },
    );
    let mut patch = keep.then(|| one("proposals", &proposal, value));
    if let (Some(patch), Some(task)) = (patch.as_mut(), task.as_deref()) {
        merge(patch, &one("tasks", task, task_value(p, by)));
    }
    Planned {
        patch,
        answer: Answer {
            proposal: keep.then_some(proposal),
            state: state.to_string(),
            task,
            owners,
            problems,
        },
    }
}

pub fn exists_in(repo: &str) -> impl Fn(&str) -> bool + '_ {
    move |file: &str| Path::new(repo).join(file).exists()
}

pub async fn save_with<F>(
    persist: &PersistHandle,
    fleet: Uuid,
    mut build: F,
) -> Result<Option<Answer>, String>
where
    F: FnMut(&FleetDefinition) -> Result<(Option<Value>, Answer), String>,
{
    for _ in 0..SAVE_ATTEMPTS {
        let row = persist
            .fleet()
            .find_fleet(&fleet.to_string())
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("fleet {fleet} does not exist"))?;
        let def = FleetDefinition::from_value(&row.definition)?;
        let (patch, answer) = build(&def)?;
        let Some(patch) = patch else {
            return Ok(Some(answer));
        };
        let mut definition = row.definition.clone();
        merge(&mut definition, &patch);
        let saved = persist
            .fleet()
            .save_fleet_definition(fleet, &definition, &row.missing, row.updated_at)
            .await
            .map_err(|e| e.to_string())?;
        if saved.is_some() {
            return Ok(Some(answer));
        }
    }
    Ok(None)
}

async fn finished_tasks(persist: &PersistHandle, fleet: Uuid) -> BTreeSet<String> {
    let Ok(Some(row)) = persist.fleet().find_fleet(&fleet.to_string()).await else {
        return BTreeSet::new();
    };
    let (Some(run), Ok(def)) = (
        row.run_id.as_deref(),
        FleetDefinition::from_value(&row.definition),
    ) else {
        return BTreeSet::new();
    };
    let events = persist
        .fleet()
        .fleet_events_since(run, 0)
        .await
        .unwrap_or_default();
    super::fold::fold(&events, DateTime::<Utc>::MAX_UTC).done_tasks(&def)
}

pub async fn propose(
    persist: &PersistHandle,
    fleet: Uuid,
    by: &str,
    p: &Proposal,
    causes: &[String],
) -> Result<Answer, String> {
    let done = finished_tasks(persist, fleet).await;
    let saved = save_with(persist, fleet, |def| {
        let judged = judge_done(def, p, &exists_in(&def.repo), &done);
        let planned = plan(def, p, judged, (by, causes));
        Ok((planned.patch, planned.answer))
    })
    .await?;
    let answer = saved.ok_or_else(|| {
        "the fleet definition kept changing while the proposal was saved; send it again".to_string()
    })?;
    if answer.state == HELD {
        ask_owners(persist, fleet, by, p, &answer).await;
    }
    Ok(answer)
}

pub fn session_of(value: Option<&str>) -> Option<Uuid> {
    value.and_then(|s| s.trim().parse().ok())
}

pub fn owner_question(
    fleet: Uuid,
    by: &str,
    p: &Proposal,
    (task, files): (&str, &[String]),
    proposal: &str,
) -> String {
    format!(
        "[mike proposal] {by} proposes a new task, `{name}`: {why}\nIt names files your task {task} owns: {files}.\nThe proposal ({proposal}) waits for your answer; nothing is refused and nothing is added until then. Settle this question with answer or decline, and act on it:\n- To let the new task have them, remove them from your task's files: define_fleet fleet={fleet} patch {{\"tasks\": {{\"{task}\": {{\"files\": [the files you keep]}}}}}}. mike adds the task on its next pass and starts a seat for it.\n- To keep them, decline: define_fleet fleet={fleet} patch {{\"proposals\": {{\"{proposal}\": {{\"state\": \"declined\"}}}}}}.",
        name = p.name.trim(),
        why = p.why.trim(),
        files = files.join(", "),
    )
}

async fn ask_owners(persist: &PersistHandle, fleet: Uuid, by: &str, p: &Proposal, answer: &Answer) {
    let (Some(proposal), Ok(Some(row))) = (
        answer.proposal.as_deref(),
        persist.fleet().find_fleet(&fleet.to_string()).await,
    ) else {
        return;
    };
    let Ok(def) = FleetDefinition::from_value(&row.definition) else {
        return;
    };
    let Some((asker, owners)) = question_recipients(&def, by, answer) else {
        return;
    };
    for owner in owners {
        let question = owner_question(fleet, by, p, (&owner.task, &owner.files), proposal);
        let asked = AskedQuestion {
            asker,
            to_session: owner.to,
            to_task: Some(&owner.task),
            run_id: row.run_id.as_deref(),
            about_note: None,
            question: &question,
            blocked: false,
        };
        if let Err(err) = super::questions::ask_and_mail(persist, &asked, None).await {
            tracing::warn!(task = %owner.task, error = %err, "fleet: could not ask a task's seat about a proposal that names its files");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerToAsk {
    pub task: String,
    pub files: Vec<String>,
    pub to: Option<Uuid>,
}

pub fn question_recipients(
    def: &FleetDefinition,
    by: &str,
    answer: &Answer,
) -> Option<(Uuid, Vec<OwnerToAsk>)> {
    let coordinator = session_of(def.coordinator.session.as_deref());
    let asker = session_of(Some(by)).or(coordinator)?;
    let owners = answer
        .owners
        .iter()
        .map(|(task, files)| OwnerToAsk {
            task: task.clone(),
            files: files.clone(),
            to: def
                .tasks
                .get(task)
                .and_then(|t| session_of(t.session.as_deref()))
                .or(coordinator),
        })
        .collect();
    Some((asker, owners))
}

pub fn answer_or_refusal(answer: Answer) -> Result<Answer, String> {
    if answer.problems.is_empty() {
        Ok(answer)
    } else {
        Err(format!(
            "the proposal was not added: {}",
            answer.problems.join("; ")
        ))
    }
}
