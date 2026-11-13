use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::status::FleetStatus;
use super::tell::Notice;
use super::watch::git;

const FAIL: &str = "fail";
const WORKING: &str = "working";
const RECENT_MINUTES: i64 = 10;
const SEAT_TRAILER: &str = "--format=%(trailers:key=Fleet-Seat,valueonly)";

pub fn reference(def: &FleetDefinition) -> Option<Uuid> {
    if !def.reference.enabled {
        return None;
    }
    def.reference.session.as_deref()?.trim().parse().ok()
}

fn seats_of(def: &FleetDefinition, tasks: &[String]) -> String {
    let named: Vec<String> = tasks
        .iter()
        .filter_map(|id| def.tasks.get(id))
        .filter_map(|task| {
            Some(format!(
                "{} (session {})",
                task.seat.as_deref()?,
                task.session.as_deref()?
            ))
        })
        .collect();
    if named.is_empty() {
        "a seat that has not started yet".to_string()
    } else {
        named.join(", ")
    }
}

pub fn on_a_fail(def: &FleetDefinition, status: &FleetStatus) -> Vec<Notice> {
    let Some(to) = reference(def) else {
        return Vec::new();
    };
    let recent = status.now - Duration::minutes(RECENT_MINUTES);
    status
        .verdicts
        .iter()
        .filter(|verdict| verdict.verdict == FAIL && verdict.at >= recent)
        .map(|verdict| Notice {
            to,
            key: format!(
                "fail:{}:{}",
                verdict.at.timestamp_micros(),
                verdict.tasks.join("+")
            ),
            covers: Vec::new(),
            body: format!(
                "[mike reference] Acceptance failed {} at {}: \"{}\". The work is done by {}. Read what the failure names in the tree as it is now (list_fleet_changes for those paths, then the files; fleet_where for a name) and mail that seat what you find, with file and line: where the thing is, how it works, what another seat changed there. If the failure is not about where something is or how something works, do nothing.",
                verdict.tasks.join(", "),
                verdict.at.format("%H:%M:%SZ"),
                verdict.title,
                seats_of(def, &verdict.tasks)
            ),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quiet {
    pub task: String,
    pub name: String,
    pub seat: String,
    pub session: String,
    pub minutes: i64,
    pub files: Vec<String>,
}

pub fn quiet_seats(
    def: &FleetDefinition,
    status: &FleetStatus,
    committed: &BTreeSet<String>,
) -> Vec<Quiet> {
    let Some(quiet_after) = def.reference.quiet_minutes else {
        return Vec::new();
    };
    status
        .tasks
        .iter()
        .filter(|task| task.state == WORKING)
        .filter_map(|task| {
            let seat = task.seat.as_deref()?;
            let open = status.seats.iter().find(|s| s.seat == seat && !s.closed)?;
            let opened = open.opened_at?;
            let minutes = (status.now - opened).num_minutes();
            let defined = def.tasks.get(&task.id)?;
            (minutes >= quiet_after && !committed.contains(seat)).then(|| Quiet {
                task: task.id.clone(),
                name: task.name.clone(),
                seat: seat.to_string(),
                session: open.session.map(|s| s.to_string()).unwrap_or_default(),
                minutes,
                files: defined.files.clone(),
            })
        })
        .collect()
}

fn quiet_key(seat: &str) -> String {
    format!("no-commit:{seat}")
}

pub fn on_no_commit(
    def: &FleetDefinition,
    status: &FleetStatus,
    committed: &BTreeSet<String>,
    said: impl Fn(&str) -> bool,
) -> Option<Notice> {
    let to = reference(def)?;
    let fresh: Vec<Quiet> = quiet_seats(def, status, committed)
        .into_iter()
        .filter(|quiet| !said(&quiet_key(&quiet.seat)))
        .collect();
    if fresh.is_empty() {
        return None;
    }
    let listed: Vec<String> = fresh
        .iter()
        .map(|q| {
            format!(
                "- {} (session {}), task {}: {}, {} min, files {}",
                q.seat,
                q.session,
                q.task,
                q.name,
                q.minutes,
                q.files.join(", ")
            )
        })
        .collect();
    let keys: Vec<String> = fresh.iter().map(|q| quiet_key(&q.seat)).collect();
    Some(Notice {
        to,
        key: keys.join("+"),
        covers: keys,
        body: format!(
            "[mike reference] These seats have worked for ten minutes or more without a commit:\n{}\nFor each, read what it has changed so far (list_fleet_changes with its files as `path`) and, if you can see what it needs to know (where something is defined, how a part works, what another seat changed there, a problem a seat has already hit), mail it now with file and line. If you cannot tell what a seat is missing, mail it one question: what are you looking for? You never do the seat's work.",
            listed.join("\n")
        ),
    })
}

pub fn seats_in_trailers(log: &str) -> BTreeSet<String> {
    log.lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

pub async fn committed_seats(repo: &str, since: Option<DateTime<Utc>>) -> BTreeSet<String> {
    let since = since
        .map(|at| format!("--since={}", at.to_rfc3339()))
        .unwrap_or_else(|| "--since=1 day ago".to_string());
    git(repo, &["log", &since, SEAT_TRAILER], None)
        .await
        .map(|raw| seats_in_trailers(&String::from_utf8_lossy(&raw)))
        .unwrap_or_default()
}

#[cfg(test)]
pub mod tests {
    use serde_json::json;

    use super::*;
    use crate::launch::fleet::status::{SeatStatus, TaskStatus, Verdict};
    use crate::launch::fleet::tests::changed;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap_or_default()
    }

    fn def() -> FleetDefinition {
        changed(json!({
            "reference": {"enabled": true, "session": Uuid::from_u128(40).to_string(), "quiet_minutes": 10},
            "tasks": {
                "P1": {"seat": "db-dev-P1", "session": Uuid::from_u128(11).to_string()},
                "P2": {"seat": "frontend-developer-P2", "session": Uuid::from_u128(12).to_string()}
            }
        }))
    }

    fn task(id: &str, seat: &str, state: &str) -> TaskStatus {
        TaskStatus {
            id: id.into(),
            name: format!("{id} work"),
            discipline: "db-dev".into(),
            criteria: vec![],
            seat: Some(seat.into()),
            last_seat: None,
            state: state.into(),
            passes: 0,
            fails: 0,
            rounds: 0,
            last: None,
            passed_criteria: vec![],
            ready: false,
        }
    }

    fn seat(name: &str, opened: &str) -> SeatStatus {
        SeatStatus {
            seat: name.into(),
            role: "db-dev".into(),
            model: "sonnet".into(),
            state: "working".into(),
            session: Some(Uuid::from_u128(if name == "db-dev-P1" { 11 } else { 12 })),
            goal: None,
            context_tokens: None,
            requests: 3,
            last_request_at: None,
            opened_at: Some(at(opened)),
            closed: false,
            first_request_cache: None,
        }
    }

    fn status() -> FleetStatus {
        FleetStatus {
            fleet: "f".into(),
            slug: "s".into(),
            state: "running".into(),
            run_id: Some("fleet-s-1".into()),
            goal: String::new(),
            started_at: Some(at("2026-10-01T12:40:00Z")),
            finished_at: None,
            now: at("2026-10-01T13:00:00Z"),
            missing: vec![],
            criteria: vec![],
            tasks: vec![
                task("P1", "db-dev-P1", "working"),
                task("P2", "frontend-developer-P2", "working"),
            ],
            seats: vec![
                seat("db-dev-P1", "2026-10-01T12:44:00Z"),
                seat("frontend-developer-P2", "2026-10-01T12:55:00Z"),
            ],
            verdicts: vec![],
            blocked: vec![],
            needs_operator: vec![],
            accepted: false,
            obligations: vec![],
            every_seat_idle_seconds: None,
            tree_broken: None,
            problems: Vec::new(),
        }
    }

    #[test]
    fn no_seat_is_called_quiet_unless_the_definition_sets_quiet_minutes() {
        let mut unset = def();
        unset.reference.quiet_minutes = None;
        assert!(quiet_seats(&unset, &status(), &BTreeSet::new()).is_empty());
        assert_eq!(
            on_no_commit(&unset, &status(), &BTreeSet::new(), |_| false),
            None
        );
    }

    #[test]
    fn the_reference_seat_is_asked_about_a_seat_ten_minutes_in_with_no_commit() {
        let nobody = BTreeSet::new();
        let Some(asked) = on_no_commit(&def(), &status(), &nobody, |_| false) else {
            panic!("a seat sixteen minutes in with no commit is asked about");
        };
        assert_eq!(asked.to, Uuid::from_u128(40));
        assert_eq!(asked.covers, vec!["no-commit:db-dev-P1".to_string()]);
        assert!(
            asked.body.contains(&format!(
                "- db-dev-P1 (session {}), task P1: P1 work, 16 min, files ",
                Uuid::from_u128(11)
            )),
            "{}",
            asked.body
        );
        assert!(
            !asked.body.contains("frontend-developer-P2"),
            "five minutes in is not yet quiet"
        );
        let committed =
            seats_in_trailers("db-dev-P1 (0000-11)\n\nfrontend-developer-P2 (0000-12)\n");
        assert_eq!(on_no_commit(&def(), &status(), &committed, |_| false), None);
        assert_eq!(
            on_no_commit(&def(), &status(), &nobody, |key| key
                == "no-commit:db-dev-P1"),
            None,
            "a seat is asked about once"
        );
    }

    #[test]
    fn the_reference_seat_is_sent_a_fresh_fail_with_the_seat_that_did_the_work() {
        let mut failed = status();
        failed.verdicts = vec![
            Verdict {
                at: at("2026-10-01T12:58:00Z"),
                verdict: "fail".into(),
                title: "P1 slice 2: the marker is read from the wrong position".into(),
                tasks: vec!["P1".into()],
                criteria: vec![],
            },
            Verdict {
                at: at("2026-10-01T12:59:00Z"),
                verdict: "pass".into(),
                title: "P2 slice 1".into(),
                tasks: vec!["P2".into()],
                criteria: vec![],
            },
            Verdict {
                at: at("2026-10-01T12:41:00Z"),
                verdict: "fail".into(),
                title: "an old one".into(),
                tasks: vec!["P2".into()],
                criteria: vec![],
            },
        ];
        let asked = on_a_fail(&def(), &failed);
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked[0].body.starts_with(
            "[mike reference] Acceptance failed P1 at 12:58:00Z: \"P1 slice 2: the marker is read from the wrong position\". The work is done by db-dev-P1 (session "
        ), "{}", asked[0].body);
        let off = changed(json!({"reference": {"enabled": false}}));
        assert!(on_a_fail(&off, &failed).is_empty());
    }
}
