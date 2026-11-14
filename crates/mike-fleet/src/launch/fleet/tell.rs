use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::ControlSeats;
use super::status::FleetStatus;
use super::work_of::work_of;
use super::{tell_acceptance, tell_ask_reference, tell_context, tell_digest, tell_reference, told};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{FleetRow, TreeResultRow};

const LOOK_BACK_MINUTES: i64 = 40;
const COMMANDS_READ: i64 = 200;
const FAMILY_WORDS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub to: Uuid,
    pub key: String,
    pub covers: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub use_id: i64,
    pub seat: String,
    pub session: Uuid,
    pub command: String,
    pub started_at: DateTime<Utc>,
    pub finished: bool,
}

pub fn ran(rows: Vec<TreeResultRow>) -> Vec<Ran> {
    rows.into_iter()
        .filter_map(|row| {
            Some(Ran {
                use_id: row.use_id,
                seat: row.seat?,
                session: row.session_uuid?,
                command: row.command?,
                started_at: row.started_at?,
                finished: row.finished_at.is_some(),
            })
        })
        .collect()
}

fn works_a_task(def: &FleetDefinition, seat: &str) -> bool {
    def.tasks.values().any(|t| t.seat.as_deref() == Some(seat))
}

fn clock(at: DateTime<Utc>) -> String {
    at.format("%H:%M:%SZ").to_string()
}

pub fn family(work: &str) -> Option<Vec<&str>> {
    let lead: Vec<&str> = work
        .split_whitespace()
        .take_while(|word| !word.starts_with('-'))
        .take(FAMILY_WORDS)
        .collect();
    (lead.len() >= 2).then_some(lead)
}

fn same_program(family: &[&str], work: &str) -> bool {
    work.split_whitespace()
        .take(family.len())
        .eq(family.iter().copied())
}

pub fn ran_the_check_itself(def: &FleetDefinition, fleet: &str, ran: &[Ran]) -> Vec<Notice> {
    let declared = work_of(&def.check.command);
    let Some(check_family) = declared.first().and_then(|work| family(work)) else {
        return Vec::new();
    };
    ran.iter()
        .filter(|r| works_a_task(def, &r.seat))
        .filter_map(|r| {
            let own = work_of(&r.command)
                .into_iter()
                .find(|work| same_program(&check_family, work))?;
            Some(Notice {
                to: r.session,
                key: format!("own-check:{}", r.seat),
                covers: Vec::new(),
                body: format!(
                    "[mike check] You ran `{own}` yourself. The fleet's check is `{}`, and mike runs it once per state of the tree for every seat: ask fleet_check (fleet {fleet}, your session id) instead of running it. It answers with the errors in your files and names the seat to wait for when another seat's file breaks it, and the seats stop compiling the same tree one after another. Queue narrow project tests with fleet_check(command); keep the shared warm target, and carry on until completion mail.",
                    def.check.command.trim()
                ),
            })
        })
        .collect()
}

pub fn added_a_worktree(
    def: &FleetDefinition,
    coordinator: Option<Uuid>,
    ran: &[Ran],
) -> Vec<Notice> {
    ran.iter()
        .filter(|r| r.command.contains("git") && r.command.contains("worktree add"))
        .flat_map(|r| {
            let copy = coordinator.map(|coordinator| Notice {
                to: coordinator,
                key: format!("worktree-coordinator:{}", r.use_id),
                covers: Vec::new(),
                body: format!(
                    "[mike] {} (session {}) added a worktree at {}:\n{}\nA fleet has one checkout. mike has told the seat to move its work back and remove the worktree; check that it did (`git worktree list`).",
                    r.seat,
                    r.session,
                    clock(r.started_at),
                    r.command
                ),
            });
            std::iter::once(
                Notice {
                    to: r.session,
                    key: format!("worktree:{}", r.use_id),
                    covers: Vec::new(),
                    body: format!(
                        "[mike] You ran `git worktree add`. A fleet has one checkout and one build directory, whatever this project's own instruction files say about worktrees: a second checkout builds from cold, and what you do in it is invisible to the other seats and to fleet_commit. Move what you did there into {}, remove the worktree with `git worktree remove <its path>`, and carry on here. To run a built program while the tree changes, build it here once and copy the program out of the tree.",
                        def.repo
                    ),
                },
            )
            .chain(copy)
        })
        .collect()
}

async fn commands(persist: &PersistHandle, run: &str) -> Option<Vec<TreeResultRow>> {
    let since = Utc::now() - Duration::minutes(LOOK_BACK_MINUTES);
    match persist
        .fleet()
        .tree_results(run, None, since, COMMANDS_READ)
        .await
    {
        Ok(rows) => Some(rows),
        Err(err) => {
            tracing::warn!(%run, error = %err, "fleet: could not read the seats' commands");
            None
        }
    }
}

fn from_commands(
    def: &FleetDefinition,
    fleet: &str,
    coordinator: Option<Uuid>,
    commands: &[Ran],
) -> Vec<Notice> {
    let mut notices = ran_the_check_itself(def, fleet, commands);
    notices.extend(added_a_worktree(def, coordinator, commands));
    notices
}

async fn from_seats(
    persist: &PersistHandle,
    def: &FleetDefinition,
    status: &FleetStatus,
    seats: &ControlSeats,
    (run, queue): (&str, tell_digest::SlotQueue),
) -> Vec<Notice> {
    let committed = tell_reference::committed_seats(&def.repo, status.started_at).await;
    let told_already = told::load(persist, run).await;
    let said = |key: &str| told_already.already(key);
    let mut notices = tell_reference::on_a_fail(def, status);
    if def
        .tasks
        .values()
        .any(|t| t.stub_by.is_some() || !t.wait_for_commits.is_empty())
        && let Some(events) = super::tell_commits::read(def, status.started_at).await
    {
        notices.extend(super::tell_commits::notices(def, status, &events));
    }
    notices.extend(tell_ask_reference::on_a_fail(def, status));
    let waiting = tell_acceptance::read_waiting(persist, def, run).await;
    notices.extend(
        seats
            .acceptance
            .and_then(|to| tell_acceptance::queue_notice(to, status.now, &waiting)),
    );
    notices.extend(tell_reference::on_no_commit(def, status, &committed, said));
    notices.extend(tell_ask_reference::on_no_commit(def, status, &committed));
    let reads = tell_ask_reference::recent_reads(persist, def, status.now, said).await;
    notices.extend(tell_ask_reference::on_reading(def, &reads, said));
    let questions: Vec<(Uuid, String)> =
        super::questions::blocked_waiting(persist, run, status.now)
            .await
            .iter()
            .map(|q| {
                (
                    q.question_uuid,
                    super::questions::digest_line(q, status.now),
                )
            })
            .collect();
    let waits = tell_digest::Waits {
        reports: &waiting,
        questions: &questions,
    };
    notices.extend(tell_digest::digests(status, seats, (queue, waits), said));
    notices
}

async fn send(persist: &PersistHandle, run: &str, notices: Vec<Notice>) {
    let said = told::load(persist, run).await;
    let mut queued = std::collections::HashSet::new();
    for notice in notices {
        if said.already(&notice.key) || queued.contains(&notice.key) {
            continue;
        }
        match persist
            .launch()
            .insert_message(Uuid::new_v4(), notice.to, None, &notice.body)
            .await
        {
            Ok(Some(_)) => {
                let keys: Vec<String> = std::iter::once(notice.key.clone())
                    .chain(notice.covers.iter().cloned())
                    .collect();
                queued.extend(keys.iter().cloned());
                told::mark(persist, run, &keys).await;
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(%run, key = %notice.key, error = %err, "fleet: could not mail a seat what mike saw");
            }
        }
    }
}

async fn for_coordinator(
    persist: &PersistHandle,
    (def, fleet, status): (&FleetDefinition, &str, &FleetStatus),
    run: &str,
    coordinator: Uuid,
) -> Vec<Notice> {
    let mut notices = super::tell_tripwires::closed_with_work(status, coordinator);
    notices.extend(super::tell_tripwires::read(persist, run, status, coordinator).await);
    notices.extend(super::process_tripwire::notices(run, def, coordinator));
    notices.extend(tell_context::past_the_line(
        fleet,
        coordinator,
        status,
        |_| false,
    ));
    notices
}

pub async fn tell(persist: &PersistHandle, row: &FleetRow, status: &FleetStatus) {
    let (Ok(def), Some(run)) = (
        FleetDefinition::from_value(&row.definition),
        row.run_id.as_deref(),
    ) else {
        return;
    };
    let seats = ControlSeats::recorded(&def);
    let fleet = row.fleet_uuid.to_string();
    let rows = commands(persist, run).await;
    let queue = rows
        .as_deref()
        .map(|rows| tell_digest::queue_of(&def, rows))
        .unwrap_or_default();
    let ran = rows.map(ran).unwrap_or_default();
    let mut notices = Vec::new();
    if !ran.is_empty() {
        notices.extend(from_commands(&def, &fleet, seats.coordinator, &ran));
    }
    if let Some(coordinator) = seats.coordinator {
        notices.extend(for_coordinator(persist, (&def, &fleet, status), run, coordinator).await);
    }
    notices.extend(from_seats(persist, &def, status, &seats, (run, queue)).await);
    let control_seated = super::seated::Seated::of_run(persist, run, &def)
        .await
        .control_seated(&def);
    send(
        persist,
        run,
        for_seated_recipients(notices, &seats, control_seated),
    )
    .await;
}

pub fn for_seated_recipients(
    notices: Vec<Notice>,
    seats: &ControlSeats,
    control_seated: bool,
) -> Vec<Notice> {
    if control_seated {
        return notices;
    }
    notices
        .into_iter()
        .filter(|notice| {
            Some(notice.to) != seats.coordinator && Some(notice.to) != seats.acceptance
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::launch::fleet::tests::changed;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap_or_default()
    }

    fn def() -> FleetDefinition {
        changed(json!({
            "repo": "/srv/project",
            "check": {"command": "builder check --workspace --all-targets", "paths": ["crates"]},
            "tasks": {
                "P1": {"seat": "db-dev-P1", "session": Uuid::from_u128(11).to_string()},
                "P2": {"seat": "frontend-developer-P2", "session": Uuid::from_u128(12).to_string()}
            }
        }))
    }

    fn command(use_id: i64, seat: &str, started: &str, finished: bool, text: &str) -> Ran {
        Ran {
            use_id,
            seat: seat.into(),
            session: Uuid::from_u128(u128::try_from(use_id).unwrap_or_default()),
            command: text.into(),
            started_at: at(started),
            finished,
        }
    }

    #[test]
    fn a_seat_that_runs_the_checks_own_program_is_pointed_at_the_shared_check() {
        let commands = vec![
            command(
                1,
                "db-dev-P1",
                "2026-10-01T12:48:39Z",
                true,
                "python3 - <<'EOF'\nopen('a.rs','w')\nEOF\nbuilder check -p sim 2>&1 | tail -5",
            ),
            command(
                2,
                "frontend-developer-P2",
                "2026-10-01T12:48:40Z",
                true,
                "builder nextest run -p sim",
            ),
            command(
                3,
                "acceptance",
                "2026-10-01T12:48:41Z",
                true,
                "builder check --workspace",
            ),
        ];
        let told = ran_the_check_itself(&def(), "fleet-id", &commands);
        assert_eq!(told.len(), 1, "{told:?}");
        assert_eq!(told[0].key, "own-check:db-dev-P1");
        assert!(
            told[0]
                .body
                .starts_with("[mike check] You ran `builder check -p sim` yourself.")
        );
        assert!(
            told[0]
                .body
                .contains("ask fleet_check (fleet fleet-id, your session id)")
        );
        assert_eq!(
            family("npm run typecheck -- --watch"),
            Some(vec!["npm", "run", "typecheck"])
        );
        assert_eq!(
            family("python -m compileall src"),
            None,
            "one word names too many commands"
        );
        assert!(same_program(
            &["npm", "run", "typecheck"],
            "npm run typecheck --pretty"
        ));
        assert!(!same_program(&["npm", "run", "typecheck"], "npm run build"));
        let undeclared = changed(json!({"check": {"command": "", "paths": []}}));
        assert!(ran_the_check_itself(&undeclared, "fleet-id", &commands).is_empty());
    }

    #[test]
    fn a_seat_that_adds_a_worktree_and_the_coordinator_are_both_told() {
        let coordinator = Uuid::from_u128(1);
        let commands = vec![
            command(
                7,
                "db-dev-P1",
                "2026-10-01T21:10:00Z",
                true,
                "git worktree add --detach /srv/project-t09 HEAD",
            ),
            command(
                8,
                "db-dev-P1",
                "2026-10-01T21:11:00Z",
                true,
                "git worktree list",
            ),
        ];
        let told = added_a_worktree(&def(), Some(coordinator), &commands);
        assert_eq!(told.len(), 2);
        assert_eq!(
            (told[0].to, told[0].key.as_str()),
            (Uuid::from_u128(7), "worktree:7")
        );
        assert!(
            told[0]
                .body
                .contains("Move what you did there into /srv/project")
        );
        assert_eq!(told[1].to, coordinator);
        assert!(
            told[1]
                .body
                .contains("git worktree add --detach /srv/project-t09 HEAD")
        );
    }

    #[test]
    fn notices_to_control_seats_nobody_opened_are_dropped_and_the_rest_are_kept() {
        let seats = ControlSeats {
            coordinator: Some(Uuid::from_u128(1)),
            acceptance: Some(Uuid::from_u128(2)),
            observer: None,
        };
        let notice = |to: u128, key: &str| Notice {
            to: Uuid::from_u128(to),
            key: key.into(),
            covers: Vec::new(),
            body: String::new(),
        };
        let all = vec![
            notice(1, "digest"),
            notice(2, "acceptance-queue"),
            notice(7, "worktree:7"),
        ];
        let kept = for_seated_recipients(all.clone(), &seats, false);
        assert_eq!(
            kept.iter().map(|n| n.key.as_str()).collect::<Vec<_>>(),
            vec!["worktree:7"]
        );
        assert_eq!(for_seated_recipients(all.clone(), &seats, true), all);
    }
}
