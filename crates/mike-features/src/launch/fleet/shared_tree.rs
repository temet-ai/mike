use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::diagnostics::{self, Diagnostic};
use super::events::{EventKind, EventPayload, FleetEvent, NewFleetEvent, TreeBrokenBy};
use super::tell::Notice;
use crate::base_state::BaseState;
use crate::store::persistence::PersistHandle;

const HELD_FOR_MINUTES: i64 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeBroken {
    pub since: DateTime<Utc>,
    pub tasks: Vec<String>,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Held {
    pub since: Option<DateTime<Utc>>,
    pub by_task: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub who: String,
    pub session: Option<Uuid>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Roster {
    pub sessions: BTreeMap<String, Uuid>,
    pub reference: Option<Uuid>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub events: Vec<NewFleetEvent>,
    pub notices: Vec<Notice>,
}

pub fn held(events: &[FleetEvent], until: DateTime<Utc>) -> Held {
    let mut ordered: Vec<&FleetEvent> = events
        .iter()
        .filter(|e| e.kind == EventKind::TreeBrokenBy && e.occurred_at <= until)
        .collect();
    ordered.sort_by_key(|e| (e.occurred_at, e.id));
    let mut now = Held::default();
    for event in ordered {
        let Ok(body) = serde_json::from_value::<TreeBrokenBy>(event.payload.clone()) else {
            continue;
        };
        match &event.task {
            None if body.paths.is_empty() => now.by_task.clear(),
            None => continue,
            Some(task) if body.paths.is_empty() => {
                now.by_task.remove(task);
            }
            Some(task) => {
                now.by_task.insert(task.clone(), body.paths);
            }
        }
        if now.by_task.is_empty() {
            now.since = None;
        } else if now.since.is_none() {
            now.since = Some(event.occurred_at);
        }
    }
    now
}

pub fn state(events: &[FleetEvent], until: DateTime<Utc>) -> Option<TreeBroken> {
    let now = held(events, until);
    let since = now.since?;
    let paths: BTreeSet<String> = now.by_task.values().flatten().cloned().collect();
    Some(TreeBroken {
        since,
        tasks: now.by_task.keys().cloned().collect(),
        paths: paths.into_iter().collect(),
    })
}

pub fn minutes(since: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - since).num_minutes().max(0)
}

pub fn line(broken: &TreeBroken, now: DateTime<Utc>) -> String {
    let minutes = minutes(broken.since, now);
    let unit = if minutes == 1 { "minute" } else { "minutes" };
    format!(
        "the tree has not compiled for {minutes} {unit}; the failing lines are in files {} have not committed",
        join_tasks(&broken.tasks)
    )
}

fn join_tasks(tasks: &[String]) -> String {
    match tasks {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn task_of(
    def: &FleetDefinition,
    roster: &Roster,
    author: Option<&Author>,
    path: &str,
) -> Option<String> {
    let by_author = author.and_then(|a| {
        let session = a.session?;
        roster
            .sessions
            .iter()
            .find(|(_, s)| **s == session)
            .map(|(task, _)| task.clone())
    });
    by_author.or_else(|| super::commit_paths::owner_of(def, path).map(|(id, _)| id.clone()))
}

pub fn attribute(
    def: &FleetDefinition,
    roster: &Roster,
    found: &[Diagnostic],
    uncommitted: &BTreeMap<String, Option<Author>>,
) -> BTreeMap<String, Vec<Diagnostic>> {
    let mut by_task: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();
    for d in found {
        let Some(author) = uncommitted.get(&d.path) else {
            continue;
        };
        if let Some(task) = task_of(def, roster, author.as_ref(), &d.path) {
            by_task.entry(task).or_default().push(d.clone());
        }
    }
    by_task
}

fn paths_of(lines: &[Diagnostic]) -> Vec<String> {
    let set: BTreeSet<&str> = lines.iter().map(|d| d.path.as_str()).collect();
    set.into_iter().map(str::to_owned).collect()
}

fn event(run: &str, task: Option<&str>, sha: &str, paths: Vec<String>) -> NewFleetEvent {
    NewFleetEvent {
        run_id: run.to_owned(),
        task: task.map(str::to_owned),
        session_uuid: None,
        payload: EventPayload::TreeBrokenBy(TreeBrokenBy {
            sha: sha.to_owned(),
            paths,
        }),
    }
}

pub struct Verdict<'a> {
    pub run: &'a str,
    pub sha: &'a str,
    pub now: DateTime<Utc>,
}

pub fn evaluate(
    def: &FleetDefinition,
    roster: &Roster,
    prior: &Held,
    broken: &BTreeMap<String, Vec<Diagnostic>>,
    at: &Verdict<'_>,
) -> Outcome {
    let mut out = Outcome::default();
    for (task, lines) in broken {
        let paths = paths_of(lines);
        if prior.by_task.get(task) != Some(&paths) {
            out.events.push(event(at.run, Some(task), at.sha, paths));
        }
    }
    for task in prior.by_task.keys().filter(|t| !broken.contains_key(*t)) {
        out.events
            .push(event(at.run, Some(task), at.sha, Vec::new()));
    }
    let since = prior.since.unwrap_or(at.now);
    out.notices = broken
        .iter()
        .filter_map(|(task, lines)| owner_notice(def, roster, task, lines, since))
        .collect();
    out
}

fn owner_notice(
    def: &FleetDefinition,
    roster: &Roster,
    task: &str,
    lines: &[Diagnostic],
    since: DateTime<Utc>,
) -> Option<Notice> {
    let session = *roster.sessions.get(task)?;
    let listed: Vec<String> = lines
        .iter()
        .map(|d| format!("- {}:{} {}", d.path, d.line, d.text))
        .collect();
    Some(Notice {
        to: session,
        key: format!("tree-broken:{task}:{}", since.timestamp()),
        covers: Vec::new(),
        body: format!(
            "[mike tree] `{}` fails in the shared tree at lines in files you have changed and not committed. Fix these or commit a state that compiles; every seat builds this tree:\n{}",
            def.check.command.trim(),
            listed.join("\n")
        ),
    })
}

pub fn tripwires(roster: &Roster, held: &Held, now: DateTime<Utc>) -> Vec<Notice> {
    let (Some(since), Some(to)) = (held.since, roster.reference) else {
        return Vec::new();
    };
    if minutes(since, now) < HELD_FOR_MINUTES {
        return Vec::new();
    }
    held.by_task
        .iter()
        .map(|(task, paths)| {
            let seat = roster
                .sessions
                .get(task)
                .map_or_else(|| "no seat yet".to_owned(), |s| format!("the seat (session {s})"));
            Notice {
                to,
                key: format!("tree-held:{task}:{}", since.timestamp()),
                covers: Vec::new(),
                body: format!(
                    "[mike reference] {seat} of task {task} has held the shared tree uncompilable for {} minutes with uncommitted changes in:\n{}\nRead what it changed (list_fleet_changes with those paths) and mail it what it needs to know: where something is defined, how a part works, what another seat changed there. If you cannot tell, mail it one question: what is not compiling? You never do the seat's work.",
                    minutes(since, now),
                    paths
                        .iter()
                        .map(|p| format!("- {p}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            }
        })
        .collect()
}

#[derive(Default)]
struct Last {
    fingerprint: Option<String>,
    running: bool,
}

fn lasts() -> &'static Mutex<HashMap<String, Last>> {
    static LASTS: OnceLock<Mutex<HashMap<String, Last>>> = OnceLock::new();
    LASTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn begin(run: &str, fingerprint: &str) -> bool {
    let Ok(mut all) = lasts().lock() else {
        return false;
    };
    let last = all.entry(run.to_owned()).or_default();
    if last.running || last.fingerprint.as_deref() == Some(fingerprint) {
        return false;
    }
    last.running = true;
    true
}

fn finish(run: &str, fingerprint: &str) {
    if let Ok(mut all) = lasts().lock() {
        let last = all.entry(run.to_owned()).or_default();
        last.running = false;
        last.fingerprint = Some(fingerprint.to_owned());
    }
}

pub fn fingerprint(repo: &str, dirty: &[String]) -> String {
    dirty
        .iter()
        .map(|p| {
            let meta = std::fs::metadata(std::path::Path::new(repo).join(p)).ok();
            let stamp = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{p}:{}:{stamp}", meta.map_or(0, |m| m.len()))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn dirty_from_porcelain(porcelain: &str) -> Vec<String> {
    porcelain
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| {
            let rest = &l[3..];
            rest.rsplit(" -> ").next().unwrap_or(rest).trim_matches('"')
        })
        .map(str::to_owned)
        .collect()
}

async fn git_out(repo: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn authors(
    persist: &PersistHandle,
    repo: &str,
    dirty: &[String],
) -> BTreeMap<String, Option<Author>> {
    let mut found = BTreeMap::new();
    for path in dirty {
        let author = crate::change_record::answers::whose(persist, repo, path)
            .await
            .ok()
            .and_then(|a| a.last_change)
            .map(|c| Author {
                who: c.who,
                session: c.session,
            });
        found.insert(path.clone(), author);
    }
    found
}

async fn held_now(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
) -> Option<(Held, Roster)> {
    let events = persist.fleet().fleet_events_since(run, 0).await.ok()?;
    let now = Utc::now();
    let folded = super::fold::fold(&events, now);
    let sessions = def
        .tasks
        .keys()
        .filter_map(|task| Some((task.clone(), folded.seat_of(task)?)))
        .collect();
    let reference = super::tell_reference::reference(def);
    Some((
        held(&events, now),
        Roster {
            sessions,
            reference,
        },
    ))
}

async fn send(persist: &PersistHandle, run: &str, notices: Vec<Notice>) {
    for notice in notices {
        if super::told::already(persist, run, &notice.key).await {
            continue;
        }
        if matches!(
            persist
                .launch()
                .insert_message(Uuid::new_v4(), notice.to, None, &notice.body)
                .await,
            Ok(Some(_))
        ) {
            super::told::mark(persist, run, &[notice.key]).await;
        }
    }
}

async fn record(persist: &PersistHandle, outcome: Outcome, run: &str) {
    for event in &outcome.events {
        if let Err(err) = persist.fleet().append_fleet_event(event).await {
            tracing::warn!(%run, error = %err, "fleet: could not record who holds the tree broken");
        }
    }
    send(persist, run, outcome.notices).await;
}

async fn check_and_attribute(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    dirty: &[String],
) {
    let (code, output) = super::check_run::check_shared_tree(def, run, &def.check.command).await;
    let Some(code) = code else {
        return;
    };
    let Some((prior, roster)) = held_now(persist, def, run).await else {
        return;
    };
    let found = if code == 0 {
        Vec::new()
    } else {
        diagnostics::parse(&output, &def.repo)
    };
    let uncommitted = authors(persist, &def.repo, dirty).await;
    let broken = attribute(def, &roster, &found, &uncommitted);
    let sha = git_out(&def.repo, &["rev-parse", "HEAD"])
        .await
        .unwrap_or_default();
    let at = Verdict {
        run,
        sha: sha.trim(),
        now: Utc::now(),
    };
    record(persist, evaluate(def, &roster, &prior, &broken, &at), run).await;
}

pub async fn pass(state: &BaseState, persist: &PersistHandle, def: &FleetDefinition, run: &str) {
    if !def.check.declared() {
        return;
    }
    if let Some((prior, roster)) = held_now(persist, def, run).await {
        send(persist, run, tripwires(&roster, &prior, Utc::now())).await;
    }
    let dirty = git_out(
        &def.repo,
        &["status", "--porcelain", "--untracked-files=all"],
    )
    .await
    .map(|p| dirty_from_porcelain(&p))
    .unwrap_or_default();
    let print = fingerprint(&def.repo, &dirty);
    if !begin(run, &print) {
        return;
    }
    if dirty.is_empty() {
        let (persist, def, run) = (persist.clone(), def.clone(), run.to_owned());
        state.tracker.clone().spawn(async move {
            clear(&persist, &def, &run).await;
            finish(&run, &print);
        });
        return;
    }
    let (persist, def, run) = (persist.clone(), def.clone(), run.to_owned());
    state.tracker.clone().spawn(async move {
        check_and_attribute(&persist, &def, &run, &dirty).await;
        finish(&run, &print);
    });
}

async fn clear(persist: &PersistHandle, def: &FleetDefinition, run: &str) {
    let Some((prior, roster)) = held_now(persist, def, run).await else {
        return;
    };
    let at = Verdict {
        run,
        sha: "",
        now: Utc::now(),
    };
    let cleared = evaluate(def, &roster, &prior, &BTreeMap::new(), &at);
    record(persist, cleared, run).await;
}

#[cfg(test)]
#[path = "shared_tree_tests.rs"]
mod tests;
