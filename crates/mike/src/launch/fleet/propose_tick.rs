use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::check_run::baseline::diagnostics;
use super::checks::normal;
use super::commit::owner_of;
use super::definition::{FleetDefinition, ProposalDefinition, TaskDefinition, merge};
use super::events::{CriterionSuspect, EventPayload, FleetEvent, NewFleetEvent};
use super::propose::{
    ADDED, Answer, BY_MIKE, HELD, Judged, Proposal, REFUSED, exists_in, judge, next_task_id, one,
    propose, save_with, session_of, task_value,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{ChangedPathRow, FleetRow, TreeCheckRow};

const UNCOMMITTED_MINUTES: i64 = 10;
const LOOK_BACK_HOURS: i64 = 24;
const SUSPECT_FAILS: i32 = 3;
const CHANGED: &str = "changed";
const FAILING: &str = "failing";

pub async fn tick(persist: &PersistHandle, row: &FleetRow, def: &FleetDefinition) {
    if def.proposals.values().any(|p| p.state == HELD) {
        settle_held(persist, row.fleet_uuid).await;
    }
    if let Some(run) = row.run_id.as_deref() {
        if def.staffing.adopt_unowned {
            propose_for_unowned(persist, row.fleet_uuid, def, run).await;
        }
        mark_suspect_criteria(persist, run).await;
    }
}

pub(super) fn suspect_criteria(events: &[FleetEvent]) -> Vec<(String, i32)> {
    let mut fails: BTreeMap<String, i32> = BTreeMap::new();
    let mut marked: BTreeSet<String> = BTreeSet::new();
    for event in events {
        match event.decode() {
            Ok(EventPayload::VerdictRecorded(v)) if !v.passed => {
                *fails.entry(v.criterion).or_insert(0) += 1;
            }
            Ok(EventPayload::CriterionSuspect(s)) => {
                marked.insert(s.criterion);
            }
            _ => {}
        }
    }
    fails
        .into_iter()
        .filter(|(criterion, n)| *n >= SUSPECT_FAILS && !marked.contains(criterion))
        .collect()
}

async fn mark_suspect_criteria(persist: &PersistHandle, run: &str) {
    let events = match persist.fleet_events_since(run, 0).await {
        Ok(events) => events,
        Err(err) => {
            tracing::warn!(%run, %err, "fleet: could not read the events to judge criteria");
            return;
        }
    };
    for (criterion, failures) in suspect_criteria(&events) {
        record_suspect(persist, run, criterion, failures).await;
    }
}

async fn record_suspect(persist: &PersistHandle, run: &str, criterion: String, failures: i32) {
    let event = NewFleetEvent {
        run_id: run.to_owned(),
        task: None,
        session_uuid: None,
        payload: EventPayload::CriterionSuspect(CriterionSuspect {
            criterion: criterion.clone(),
            failures,
        }),
    };
    if let Err(err) = persist.append_fleet_event(&event).await {
        tracing::warn!(%run, %criterion, %err, "fleet: could not record criterion_suspect");
    }
}

fn as_proposal(held: &ProposalDefinition) -> Proposal {
    Proposal {
        name: held.name.clone(),
        why: held.why.clone(),
        files: held.files.clone(),
        new_files: held.new_files.clone(),
        criteria: held.criteria.clone(),
        after: held.after.clone(),
        discipline: held.discipline.clone(),
    }
}

pub fn settle_patch(
    def: &FleetDefinition,
    exists: &dyn Fn(&str) -> bool,
) -> (Option<Value>, Vec<(String, String)>) {
    let mut patch = Value::Object(Map::new());
    let mut added = Vec::new();
    let mut working = def.clone();
    for (id, held) in def.proposals.iter().filter(|(_, p)| p.state == HELD) {
        let p = as_proposal(held);
        let (state, task) = match judge(&working, &p, exists) {
            Judged::Clean => (ADDED, Some(next_task_id(&working))),
            Judged::Refused(_) => (REFUSED, None),
            Judged::Overlap(_) => continue,
        };
        let mut change = one("proposals", id, json!({"state": state, "task": task}));
        if let Some(task) = task.as_deref() {
            let value = task_value(&p, &held.by);
            merge(&mut change, &one("tasks", task, value.clone()));
            if let Ok(parsed) = serde_json::from_value::<TaskDefinition>(value) {
                working.tasks.insert(task.to_string(), parsed);
            }
            added.push((id.clone(), task.to_string()));
        }
        merge(&mut patch, &change);
    }
    let empty = patch.as_object().is_none_or(Map::is_empty);
    ((!empty).then_some(patch), added)
}

pub async fn settle_held(persist: &PersistHandle, fleet: Uuid) {
    let mut added = Vec::new();
    let saved = save_with(persist, fleet, |def| {
        let (patch, now_added) = settle_patch(def, &exists_in(&def.repo));
        added = now_added;
        Ok((patch, Answer::default()))
    })
    .await;
    if let Err(err) = saved {
        tracing::warn!(%fleet, %err, "fleet: could not settle the proposals waiting for an owner");
        return;
    }
    for (proposal, task) in added {
        tell_added(persist, fleet, &proposal, &task).await;
    }
}

async fn tell_added(persist: &PersistHandle, fleet: Uuid, proposal: &str, task: &str) {
    let Ok(Some(row)) = persist.find_fleet(&fleet.to_string()).await else {
        return;
    };
    let Ok(def) = FleetDefinition::from_value(&row.definition) else {
        return;
    };
    let by = def.proposals.get(proposal).map(|p| p.by.as_str());
    let to = session_of(by).or_else(|| session_of(def.coordinator.session.as_deref()));
    let Some(to) = to else {
        return;
    };
    let body = format!(
        "[mike proposal] The owner released the files of {proposal}; mike added it as task {task} and starts a seat for it on its next pass."
    );
    if let Err(err) = persist
        .insert_message(Uuid::new_v4(), to, None, &body)
        .await
    {
        tracing::warn!(%proposal, %err, "fleet: could not tell the proposer its task was added");
    }
}

pub fn known_causes(def: &FleetDefinition) -> BTreeSet<String> {
    def.proposals
        .values()
        .flat_map(|p| p.causes.iter().cloned())
        .collect()
}

fn unowned(def: &FleetDefinition, path: &str) -> bool {
    let shared: Vec<String> = def.shared_files.iter().map(|f| normal(f)).collect();
    owner_of(def, path).is_none() && !shared.iter().any(|s| s == path)
}

pub fn cause(kind: &str, path: &str) -> String {
    format!("{kind}:{path}")
}

pub fn quiet_unowned_changes(
    def: &FleetDefinition,
    rows: &[ChangedPathRow],
    now: DateTime<Utc>,
) -> Vec<String> {
    let mut latest: BTreeMap<&str, DateTime<Utc>> = BTreeMap::new();
    for row in rows {
        let seen = latest.entry(row.path.as_str()).or_insert(row.seen_at);
        *seen = (*seen).max(row.seen_at);
    }
    let known = known_causes(def);
    let quiet_since = now - Duration::minutes(UNCOMMITTED_MINUTES);
    latest
        .into_iter()
        .filter(|(path, seen)| *seen <= quiet_since && unowned(def, path))
        .filter(|(path, _)| !known.contains(&cause(CHANGED, path)))
        .map(|(path, _)| path.to_string())
        .collect()
}

pub fn unowned_failures(def: &FleetDefinition, check: &TreeCheckRow) -> Vec<String> {
    if check.finished_at.is_none() || check.exit_code.is_none() || check.moved() || check.passed() {
        return Vec::new();
    }
    let known = known_causes(def);
    let paths: BTreeSet<String> = diagnostics(def, check)
        .into_iter()
        .map(|d| d.path)
        .filter(|p| unowned(def, p) && !known.contains(&cause(FAILING, p)))
        .collect();
    paths.into_iter().collect()
}

fn shared_parts(a: &str, b: &str) -> usize {
    a.split('/')
        .zip(b.split('/'))
        .take_while(|(x, y)| x == y)
        .count()
}

pub fn nearest(def: &FleetDefinition, path: &str) -> (String, Vec<String>) {
    let best = def
        .tasks
        .values()
        .flat_map(|t| {
            t.files
                .iter()
                .map(move |f| (shared_parts(&normal(f), path), t))
        })
        .filter(|(parts, _)| *parts > 0)
        .max_by_key(|(parts, _)| *parts);
    match best {
        Some((_, task)) => (task.discipline.clone(), task.criteria.clone()),
        None => (
            def.workers.keys().next().cloned().unwrap_or_default(),
            def.criteria.keys().next().cloned().into_iter().collect(),
        ),
    }
}

fn wording(kind: &str, paths: &[String]) -> (String, String) {
    let list = paths.join(", ");
    if kind == FAILING {
        (
            format!("Make the check pass in files no task owns: {list}"),
            format!(
                "The project's check fails in {list}, and no task of this fleet owns these files, so no seat has been told. Find why, fix it and commit."
            ),
        )
    } else {
        (
            format!("Take care of files no task owns: {list}"),
            format!(
                "{list} changed in the checkout more than {UNCOMMITTED_MINUTES} minutes ago, no task owns them, and they are still not committed. Find out what they are for: finish and commit them with the task they belong to, or remove them if they are left over."
            ),
        )
    }
}

pub fn proposals_for(
    def: &FleetDefinition,
    kind: &str,
    paths: &[String],
    exists: &dyn Fn(&str) -> bool,
) -> Vec<(Proposal, Vec<String>)> {
    let mut groups: BTreeMap<(String, Vec<String>), Vec<String>> = BTreeMap::new();
    for path in paths {
        groups
            .entry(nearest(def, path))
            .or_default()
            .push(path.clone());
    }
    groups
        .into_iter()
        .map(|((discipline, criteria), paths)| {
            let (name, why) = wording(kind, &paths);
            let (files, new_files): (Vec<String>, Vec<String>) =
                paths.iter().cloned().partition(|p| exists(p));
            let causes = paths.iter().map(|p| cause(kind, p)).collect();
            let proposal = Proposal {
                name,
                why,
                files,
                new_files,
                criteria,
                after: Vec::new(),
                discipline,
            };
            (proposal, causes)
        })
        .collect()
}

async fn still_uncommitted(persist: &PersistHandle, repo: &str, paths: Vec<String>) -> Vec<String> {
    let mut kept = Vec::new();
    for path in paths {
        match crate::change_record::answers::whose(persist, repo, &path).await {
            Ok(answer) if !answer.committed => kept.push(path),
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(%path, %err, "fleet: could not ask the change record whether a path is committed")
            }
        }
    }
    kept
}

async fn unowned_causes(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
) -> (Vec<String>, Vec<String>) {
    let now = Utc::now();
    let rows = persist
        .tree_changed_paths_since(run, now - Duration::hours(LOOK_BACK_HOURS))
        .await
        .unwrap_or_default();
    let changed =
        still_uncommitted(persist, &def.repo, quiet_unowned_changes(def, &rows, now)).await;
    let failing = match persist.latest_tree_check(run).await {
        Ok(Some(check)) => unowned_failures(def, &check),
        _ => Vec::new(),
    };
    (changed, failing)
}

async fn propose_for_unowned(
    persist: &PersistHandle,
    fleet: Uuid,
    def: &FleetDefinition,
    run: &str,
) {
    let (changed, failing) = unowned_causes(persist, def, run).await;
    let exists = |p: &str| Path::new(&def.repo).join(p).exists();
    let mut wanted = proposals_for(def, CHANGED, &changed, &exists);
    wanted.extend(proposals_for(def, FAILING, &failing, &exists));
    for (proposal, causes) in wanted {
        propose_one(persist, fleet, def, &proposal, &causes).await;
    }
}

async fn propose_one(
    persist: &PersistHandle,
    fleet: Uuid,
    def: &FleetDefinition,
    proposal: &Proposal,
    causes: &[String],
) {
    match propose(persist, fleet, BY_MIKE, proposal, causes).await {
        Ok(answer) => tell_coordinator(persist, def, proposal, &answer).await,
        Err(err) => tracing::warn!(%fleet, %err, "fleet: could not record mike's own proposal"),
    }
}

pub fn coordinator_mail(p: &Proposal, answer: &Answer) -> String {
    let outcome = match (answer.state.as_str(), answer.task.as_deref()) {
        (ADDED, Some(task)) => {
            format!("mike added it as task {task} and starts a seat for it on its next pass.")
        }
        (HELD, _) => format!(
            "It names files task(s) {} own, so it waits for their seat's answer.",
            answer.owners.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
        _ => format!("It was not added: {}.", answer.problems.join("; ")),
    };
    format!(
        "[mike proposal] {} ({}): {}\n{}",
        p.name,
        answer.proposal.as_deref().unwrap_or("not recorded"),
        p.why,
        outcome
    )
}

async fn tell_coordinator(
    persist: &PersistHandle,
    def: &FleetDefinition,
    p: &Proposal,
    answer: &Answer,
) {
    let Some(to) = session_of(def.coordinator.session.as_deref()) else {
        return;
    };
    let body = coordinator_mail(p, answer);
    if let Err(err) = persist
        .insert_message(Uuid::new_v4(), to, None, &body)
        .await
    {
        tracing::warn!(%err, "fleet: could not tell the coordinator about mike's own proposal");
    }
}
