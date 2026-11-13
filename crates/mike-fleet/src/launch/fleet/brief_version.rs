use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::time::{MissedTickBehavior, interval};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::{self, ControlSeats, FleetFacts, REFERENCE_SEAT, TEMPLATE_SEAT_PREFIX};
use crate::base_state::BaseState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::launch::{LaunchListFilter, LaunchRow};

pub const VERSION_LINE: &str = "Task list version of this brief: ";
const INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refresh {
    Brief { to: Uuid, body: String },
    NoTask { to: Uuid, body: String },
}

impl Refresh {
    fn to(&self) -> Uuid {
        match self {
            Self::Brief { to, .. } | Self::NoTask { to, .. } => *to,
        }
    }

    fn body(&self) -> &str {
        match self {
            Self::Brief { body, .. } | Self::NoTask { body, .. } => body,
        }
    }
}

pub fn task_list_version(def: &FleetDefinition) -> String {
    let tasks: serde_json::Map<String, serde_json::Value> = def
        .tasks
        .iter()
        .map(|(id, task)| {
            let shape = json!({
                "name": task.name,
                "discipline": task.discipline,
                "criteria": task.criteria,
                "files": task.files,
                "new_files": task.new_files,
                "brief": task.brief,
                "goal": task.goal,
                "after": task.after,
                "stub_by": task.stub_by,
                "wait_for_commits": task.wait_for_commits,
            });
            (id.clone(), shape)
        })
        .collect();
    let worker_runtimes: std::collections::BTreeMap<_, _> = def
        .workers
        .iter()
        .map(|(name, worker)| (name, worker.runtime))
        .collect();
    let shape = json!({
        "runtimes": {"workers": worker_runtimes, "coordinator": def.coordinator.runtime,
            "acceptance": def.acceptance.runtime, "observer": def.observer.runtime},
        "tasks": tasks, "shared_files": def.shared_files, "criteria": def.criteria,
        "rules": def.rules, "check": def.check, "commit_check": def.commit_check,
        "gate": def.gate, "goals": def.goals,
    });
    let digest = Sha256::digest(shape.to_string().as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

pub fn stamped(prompt: &str, version: &str) -> String {
    format!("{}\n\n{VERSION_LINE}{version}", prompt.trim_end())
}

pub fn version_in(brief: &str) -> Option<&str> {
    let at = brief.rfind(VERSION_LINE)?;
    brief[at + VERSION_LINE.len()..].split_whitespace().next()
}

pub const PARTS_LINE: &str = "Brief parts given: ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GivenPart {
    pub part: String,
    pub note: Option<Uuid>,
    pub version: i64,
}

pub fn parts_line(parts: &[GivenPart]) -> String {
    let given: Vec<String> = parts
        .iter()
        .map(|p| {
            let note = p.note.map_or_else(|| "none".to_string(), |n| n.to_string());
            format!("{} {note} v{}", p.part, p.version)
        })
        .collect();
    format!("{PARTS_LINE}{}", given.join("; "))
}

pub fn parts_in(brief: &str) -> Vec<GivenPart> {
    let Some(at) = brief.rfind(PARTS_LINE) else {
        return Vec::new();
    };
    let line = brief[at + PARTS_LINE.len()..]
        .lines()
        .next()
        .unwrap_or_default();
    line.split(';').filter_map(given_part).collect()
}

fn given_part(text: &str) -> Option<GivenPart> {
    let mut words = text.split_whitespace();
    let part = words.next()?.to_string();
    let note = words.next()?;
    let version = words.next()?.strip_prefix('v')?.parse().ok()?;
    Some(GivenPart {
        part,
        note: Uuid::parse_str(note).ok(),
        version,
    })
}

pub fn task_part_version_in(brief: &str) -> Option<i64> {
    parts_in(brief)
        .into_iter()
        .find(|p| p.part == "task")
        .map(|p| p.version)
}

pub async fn version_for_run(persist: &PersistHandle, run: &str) -> Option<String> {
    let row = persist.fleet().find_fleet_by_run(run).await.ok()??;
    let def = FleetDefinition::from_value(&row.definition).ok()?;
    Some(task_list_version(&def))
}

pub fn task_of<'a>(def: &'a FleetDefinition, seat: &str) -> Option<&'a String> {
    let id = super::seated::task_named(def, seat)?;
    def.tasks.get_key_value(&id).map(|(id, _)| id)
}

fn unstamped(brief: &str) -> &str {
    brief
        .rfind(VERSION_LINE)
        .map_or(brief, |at| &brief[..at])
        .trim_end()
}

fn regenerated(
    def: &FleetDefinition,
    facts: &FleetFacts,
    seat: &str,
    session: Uuid,
) -> Option<String> {
    let task = task_of(def, seat)?;
    plan::worker(def, facts, task, session).map(|p| p.prompt)
}

fn ignored(seat: &str) -> bool {
    seat == REFERENCE_SEAT || seat.starts_with(TEMPLATE_SEAT_PREFIX)
}

fn brief_mail(old: &str, new: &str, brief: &str) -> String {
    format!(
        "mike: the fleet's task list changed after your brief was written (version {old}, now \
         {new}). Your brief, written again from the current definition, follows. Where it \
         differs from the brief you started with, this one holds.\n\n{}",
        stamped(brief, new)
    )
}

fn orphan_mail(seat: &str, session: Uuid, old: &str, new: &str) -> String {
    format!(
        "mike: seat {seat} (session {session}) is still open, but the fleet's task list (version \
         {new}) has no task for it; its brief was written from version {old}. Close it with \
         `mike spawn kill --session {session}`, or give it a task in the definition."
    )
}

fn refresh_for(
    def: &FleetDefinition,
    facts: &FleetFacts,
    row: &LaunchRow,
    current: &str,
) -> Option<Refresh> {
    let (seat, session) = (row.seat.as_deref()?, row.session_uuid?);
    if row.closed_at.is_some() || ignored(seat) {
        return None;
    }
    let given = row.brief_text.as_deref()?;
    let old = version_in(given)?;
    if old == current {
        return None;
    }
    if super::brief_notes::as_notes(def) && task_of(def, seat).is_some() {
        return None;
    }
    Some(match regenerated(def, facts, seat, session) {
        Some(brief) if unstamped(&brief) == unstamped(given) => return None,
        Some(brief) => Refresh::Brief {
            to: session,
            body: brief_mail(old, current, &brief),
        },
        None => Refresh::NoTask {
            to: facts.seats.coordinator,
            body: orphan_mail(seat, session, old, current),
        },
    })
}

pub fn refreshes(
    def: &FleetDefinition,
    facts: &FleetFacts,
    launches: &[LaunchRow],
) -> Vec<(String, Refresh)> {
    let current = task_list_version(def);
    launches
        .iter()
        .filter_map(|row| {
            let refresh = refresh_for(def, facts, row, &current)?;
            let seat = row.seat.as_deref().unwrap_or_default();
            Some((format!("brief:{seat}:{current}"), refresh))
        })
        .collect()
}

pub fn facts_for(row: &FleetRow, def: &FleetDefinition) -> Option<FleetFacts> {
    Some(FleetFacts {
        fleet: row.fleet_uuid.to_string(),
        slug: row.slug.clone(),
        run_id: row.run_id.clone()?,
        seats: ControlSeats::recorded(def)?,
        paths: super::bundle::paths_for(&def.repo),
    })
}

async fn refresh(persist: &PersistHandle, row: &FleetRow) {
    let Ok(def) = FleetDefinition::from_value(&row.definition) else {
        return;
    };
    let Some(facts) = facts_for(row, &def) else {
        return;
    };
    let filter = LaunchListFilter {
        run_id: Some(facts.run_id.clone()),
        open_only: true,
        ..LaunchListFilter::default()
    };
    let Ok(launches) = persist.launch().list_launches(&filter).await else {
        return;
    };
    if super::brief_notes::as_notes(&def) {
        let seats: Vec<Uuid> = launches
            .iter()
            .filter(|row| !row.seat.as_deref().is_none_or(ignored))
            .filter_map(|row| row.session_uuid)
            .collect();
        super::brief_notes_sync::sync(persist, (&def, &facts), &seats).await;
    }
    mail_refreshes(persist, &facts.run_id, refreshes(&def, &facts, &launches)).await;
}

async fn mail_refreshes(persist: &PersistHandle, run: &str, due: Vec<(String, Refresh)>) {
    let said = super::told::load(persist, run).await;
    for (key, refresh) in due.into_iter().filter(|(key, _)| !said.already(key)) {
        match persist
            .launch()
            .insert_message(Uuid::new_v4(), refresh.to(), None, refresh.body())
            .await
        {
            Ok(_) => super::told::mark(persist, run, &[key]).await,
            Err(err) => {
                tracing::warn!(%run, %key, error = %err, "fleet: could not mail a seat its brief for the changed task list");
            }
        }
    }
}

async fn tick(persist: &PersistHandle) {
    let Ok(fleets) = persist
        .fleet()
        .list_fleets_in_states(&[super::shutdown::RUNNING])
        .await
    else {
        return;
    };
    let fleets = super::here::on_this_machine(fleets, super::here::checkout_present);
    for row in &fleets {
        refresh(persist, row).await;
    }
}

pub fn spawn_brief_refresh(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    let shutdown = state.shutdown();
    state.tracker.clone().spawn(async move {
        let mut ticker = interval(INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                _ = ticker.tick() => tick(&persist).await,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::launch::fleet::definition::merge;
    use crate::launch::fleet::plan_tests::facts;
    use crate::launch::fleet::tests::{complete, parsed};

    fn launch(seat: &str, session: Uuid, brief: Option<String>) -> anyhow::Result<LaunchRow> {
        Ok(serde_json::from_value(json!({
            "id": 1, "session_uuid": session, "backend": "herdr", "folder": "/srv/project",
            "worktree": false, "seat": seat, "claude_args": [], "prompt_present": true,
            "brief_text": brief, "remote_control": true, "proxy_env": true, "opened": true,
            "confirmed": true, "result": "success", "stderr_truncated": false,
            "created_at": "2026-10-04T08:00:00Z"
        }))?)
    }

    fn seated(def: Value) -> FleetDefinition {
        let patch = json!({
            "coordinator": {"session": Uuid::from_u128(1).to_string()},
            "acceptance": {"session": Uuid::from_u128(2).to_string()},
            "observer": {"session": Uuid::from_u128(3).to_string()},
            "tasks": {"P1": {"seat": "db-dev-P1", "session": Uuid::from_u128(11).to_string()}}
        });
        let mut value = complete();
        merge(&mut value, &patch);
        merge(&mut value, &def);
        parsed(&value)
    }

    #[test]
    fn version_ignores_seat_progress_and_follows_the_task_list() {
        let before = seated(json!({}));
        let progressed = seated(json!({"tasks": {"P2": {"seat": "x", "state": "working"}}}));
        assert_eq!(task_list_version(&before), task_list_version(&progressed));
        let cut = seated(json!({"tasks": {"P10": null}}));
        assert_ne!(task_list_version(&before), task_list_version(&cut));
        let moved = seated(json!({"tasks": {"P1": {"files": ["elsewhere/"]}}}));
        assert_ne!(task_list_version(&before), task_list_version(&moved));
    }

    #[test]
    fn a_stamped_brief_reads_its_version_back() {
        let brief = stamped("do the work\n", "abc123def456");
        assert!(brief.starts_with("do the work\n\n"));
        assert_eq!(version_in(&brief), Some("abc123def456"));
        assert_eq!(version_in("an older brief with no version"), None);
    }

    #[test]
    fn stale_seats_get_their_brief_again_and_a_cut_seat_goes_to_the_coordinator()
    -> anyhow::Result<()> {
        let old_def = seated(json!({}));
        let old = task_list_version(&old_def);
        let def =
            seated(json!({"tasks": {"P10": null, "P2": {"brief": "Build the smaller page."}}}));
        let current = task_list_version(&def);
        let stale = |text: &str| Some(stamped(text, &old));
        let launches = vec![
            launch("db-dev-P1", Uuid::from_u128(11), stale("old worker brief"))?,
            launch(
                "frontend-developer-P10",
                Uuid::from_u128(12),
                stale("cut task"),
            )?,
            launch("template-db-dev", Uuid::from_u128(13), stale("template"))?,
            launch(
                "acceptance",
                Uuid::from_u128(2),
                Some(stamped("fresh", &current)),
            )?,
            launch(
                "observer",
                Uuid::from_u128(3),
                Some("before versions".into()),
            )?,
        ];

        let found = refreshes(&def, &facts(), &launches);

        assert_eq!(found.len(), 2, "{found:?}");
        let Refresh::Brief { to, body } = &found[0].1 else {
            anyhow::bail!("the worker gets its brief: {found:?}");
        };
        assert_eq!(*to, Uuid::from_u128(11));
        assert!(body.contains("Your task is P1"), "{body}");
        assert!(
            body.contains(&format!("version {old}, now {current}")),
            "{body}"
        );
        assert_eq!(version_in(body), Some(current.as_str()));
        let Refresh::NoTask { to, body } = &found[1].1 else {
            anyhow::bail!("the cut seat is reported: {found:?}");
        };
        assert_eq!(*to, Uuid::from_u128(1));
        assert!(body.contains("frontend-developer-P10"), "{body}");
        assert_eq!(found[0].0, format!("brief:db-dev-P1:{current}"));
        Ok(())
    }
    #[test]
    fn a_version_change_that_leaves_the_seats_brief_as_it_was_mails_nothing() -> anyhow::Result<()>
    {
        let old = task_list_version(&seated(json!({})));
        let def = seated(json!({"tasks": {"P2": {"brief": "Build the smaller page."}}}));
        let Some(brief) = regenerated(&def, &facts(), "db-dev-P1", Uuid::from_u128(11)) else {
            anyhow::bail!("P1 has a brief");
        };
        let launches = vec![launch(
            "db-dev-P1",
            Uuid::from_u128(11),
            Some(stamped(&brief, &old)),
        )?];
        let found = refreshes(&def, &facts(), &launches);
        assert!(found.is_empty(), "{found:?}");
        Ok(())
    }

    #[test]
    fn rule_check_and_goal_changes_invalidate_the_brief_version() {
        let before = task_list_version(&seated(json!({})));
        for patch in [
            json!({"rules": ["Run only the focused test"]}),
            json!({"check": {"command": "make check"}}),
            json!({"commit_check": [{"command": "make lint", "files": ["src"]}]}),
            json!({"gate": {"command": "make gate"}}),
            json!({"goals": {"workers": "report own evidence"}}),
            json!({"tasks": {"P1": {"goal": "report the table"}}}),
        ] {
            assert_ne!(before, task_list_version(&seated(patch.clone())), "{patch}");
        }
    }

    #[test]
    fn a_rule_only_patch_refreshes_the_open_worker() -> anyhow::Result<()> {
        let old = task_list_version(&seated(json!({})));
        let def = seated(json!({"rules": ["Use only the focused test"]}));
        let launches = vec![launch(
            "db-dev-P1",
            Uuid::from_u128(11),
            Some(stamped("old", &old)),
        )?];
        let found = refreshes(&def, &facts(), &launches);
        assert_eq!(found.len(), 1);
        let Refresh::Brief { body, .. } = &found[0].1 else {
            anyhow::bail!("expected a refreshed worker brief");
        };
        assert!(body.contains("Use only the focused test"), "{body}");
        Ok(())
    }
}
