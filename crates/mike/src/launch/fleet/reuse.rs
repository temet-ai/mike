use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use super::background_result::completion_unknown;
use super::definition::FleetDefinition;
use super::operation::Execution;
use super::work_of::{edits_files, work_through};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{ChangedPathRow, TreeResultRow};

const WINDOW_HOURS: i64 = 6;
const ROWS_READ: i64 = 200;

#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    pub command: &'a str,
    pub wrappers: &'a [String],
    pub execution: &'a Execution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReuseAnswer {
    pub answer: &'static str,
    pub work: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    pub message: String,
}

fn none(work: Vec<String>, message: &str) -> ReuseAnswer {
    ReuseAnswer {
        answer: "none",
        work,
        seat: None,
        started_at: None,
        finished_at: None,
        is_error: None,
        message: message.to_string(),
    }
}

fn from_row(answer: &'static str, work: Vec<String>, row: &TreeResultRow) -> ReuseAnswer {
    let seat = row.seat.as_deref().unwrap_or("a seat");
    let message = match answer {
        "unknown" if row.is_error == Some(true) => format!(
            "{seat} has a captured tool error. No checkout changes were observed since it started, but dependency validity and commit binding are unknown. Inspect the recorded output with list_fleet_results and include_output before deciding whether to rerun."
        ),
        "unknown" => format!(
            "{seat} has a captured result. No checkout changes were observed since it started, but dependency validity and commit binding are unknown. Tool success is not a test pass. Inspect the recorded output with list_fleet_results and include_output before deciding whether to reuse it."
        ),
        "waiting" => format!(
            "{seat} already started this command. Commands past the capacity of a declared resource may be waiting for a unit of it; this is inferred, not an observed process state. Do not queue it a second time; wait for its result."
        ),
        "unfinished" => format!(
            "{seat} launched this native background task, but its completion has not been captured. Its current process state is unknown. Read the native task and its output before reusing a result or starting it again; a launch receipt is not a passed check."
        ),
        _ => {
            format!("{seat} is running it now. Do not start it a second time; wait for its result.")
        }
    };
    ReuseAnswer {
        answer,
        work,
        seat: row.seat.clone(),
        started_at: row.started_at,
        finished_at: row.finished_at,
        is_error: row.is_error,
        message,
    }
}

fn holds(row: &TreeResultRow, wanted: &[String], wrappers: &[String]) -> bool {
    row.command.as_deref().is_some_and(|command| {
        let theirs = work_through(command, wrappers);
        wanted.iter().all(|w| theirs.contains(w))
    })
}

pub fn waiting_uses(
    rows: &[TreeResultRow],
    wrappers: &[String],
    execution: &Execution,
) -> BTreeSet<i64> {
    let mut waiting = BTreeSet::new();
    for (capacity, timed) in execution.shared_resources().values() {
        let mut open: Vec<&TreeResultRow> = rows
            .iter()
            .filter(|r| r.finished_at.is_none() && r.started_at.is_some())
            .filter(|r| !completion_unknown(r))
            .filter(|r| {
                r.command.as_deref().is_some_and(|command| {
                    timed
                        .iter()
                        .any(|(program, one)| one.covers(program, command, wrappers))
                })
            })
            .collect();
        open.sort_by_key(|r| (r.started_at, r.use_id));
        waiting.extend(open.iter().skip(*capacity).map(|r| r.use_id));
    }
    waiting
}

fn no_observed_invalidation(row: &TreeResultRow, changed: &[ChangedPathRow]) -> bool {
    row.finished_at.is_some()
        && row
            .started_at
            .is_some_and(|started| !changed.iter().any(|c| c.seen_at >= started))
}

pub fn decide(ask: &Ask<'_>, rows: &[TreeResultRow], changed: &[ChangedPathRow]) -> ReuseAnswer {
    let wanted = work_through(ask.command, ask.wrappers);
    if wanted.is_empty() {
        return none(wanted, "nothing in this command is a build or a test");
    }
    if edits_files(ask.command) {
        return none(
            wanted,
            "the command edits files before it builds, so its build is of the edited tree; run it",
        );
    }
    let matching: Vec<&TreeResultRow> = rows
        .iter()
        .filter(|r| holds(r, &wanted, ask.wrappers))
        .collect();
    if let Some(done) = matching
        .iter()
        .filter(|r| no_observed_invalidation(r, changed))
        .max_by_key(|r| r.finished_at)
    {
        return from_row("unknown", wanted, done);
    }
    let queued = waiting_uses(rows, ask.wrappers, ask.execution);
    match matching
        .iter()
        .filter(|r| r.finished_at.is_none())
        .min_by_key(|r| r.started_at)
    {
        Some(open) if completion_unknown(open) => from_row("unfinished", wanted, open),
        Some(open) if queued.contains(&open.use_id) => from_row("waiting", wanted, open),
        Some(open) => from_row("running", wanted, open),
        None => none(
            wanted,
            "no seat has a result for it at this state of the tree; run it",
        ),
    }
}

pub async fn ask(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    command: &str,
) -> Result<ReuseAnswer, String> {
    let since = Utc::now() - Duration::hours(WINDOW_HOURS);
    let rows = persist
        .tree_results(run, None, since, ROWS_READ)
        .await
        .map_err(|e| e.to_string())?;
    let changed = match rows.iter().filter_map(|r| r.started_at).min() {
        Some(earliest) => persist
            .tree_changed_paths_since(run, earliest - Duration::microseconds(1))
            .await
            .map_err(|e| e.to_string())?,
        None => Vec::new(),
    };
    if def.execution.timed_commands.is_empty() || def.execution.operation(command).is_some() {
        return Ok(declared_answer(command, &rows));
    }
    let ask = Ask {
        command,
        wrappers: &def.wrappers,
        execution: &def.execution,
    };
    Ok(decide(&ask, &rows, &changed))
}

fn declared_answer(command: &str, rows: &[TreeResultRow]) -> ReuseAnswer {
    let work = vec![command.to_string()];
    match rows
        .iter()
        .filter(|row| {
            row.command
                .as_deref()
                .is_some_and(|c| c.trim() == command.trim())
        })
        .max_by_key(|row| row.started_at)
    {
        Some(row) if row.finished_at.is_some() => from_row("unknown", work, row),
        Some(row) if completion_unknown(row) => from_row("unfinished", work, row),
        Some(row) => from_row("running", work, row),
        None => none(
            work,
            "no exact captured command matches; use fleet_check to run it and receive completion evidence",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap_or_default()
    }

    fn row(
        id: i64,
        seat: &str,
        started: &str,
        finished: Option<&str>,
        command: &str,
    ) -> TreeResultRow {
        TreeResultRow {
            use_id: id,
            seat: Some(seat.into()),
            session_uuid: None,
            command: Some(command.into()),
            started_at: Some(at(started)),
            finished_at: finished.map(at),
            is_error: Some(false),
            content: None,
        }
    }

    fn change(path: &str, seen: &str) -> ChangedPathRow {
        ChangedPathRow {
            path: path.into(),
            seen_at: at(seen),
        }
    }

    fn wrappers() -> Vec<String> {
        vec!["scripts/builder-slot".to_string()]
    }

    fn slots(capacity: u32) -> Execution {
        let timed = super::super::operation::TimedCommand {
            resources: vec![super::super::operation::ResourceClaim::Counted(
                super::super::operation::CountedResource {
                    name: "build".into(),
                    capacity,
                },
            )],
            ..Default::default()
        };
        Execution {
            timed_commands: [("builder".to_string(), timed)].into_iter().collect(),
            ..Execution::default()
        }
    }

    fn ask_for(command: &str, wrappers: &[String]) -> ReuseAnswer {
        let rows = vec![
            row(
                1,
                "sim-T02",
                "2026-10-04T10:00:00Z",
                Some("2026-10-04T10:02:00Z"),
                "python3 - <<'EOF'\nopen('a.rs','w')\nEOF\nscripts/builder-slot builder check -p sim 2>&1 | tail",
            ),
            row(
                2,
                "sim-T03",
                "2026-10-04T10:05:00Z",
                None,
                "scripts/builder-slot builder test -p sim",
            ),
        ];
        let changed = vec![change("crates/sim/src/a.rs", "2026-10-04T09:59:00Z")];
        let ask = Ask {
            command,
            wrappers,
            execution: &slots(2),
        };
        decide(&ask, &rows, &changed)
    }

    #[test]
    fn a_check_matches_the_build_part_of_a_chained_edit_and_check() {
        let answer = ask_for("scripts/builder-slot builder check -p sim", &wrappers());
        assert_eq!(answer.answer, "unknown", "{answer:?}");
        assert_eq!(answer.seat.as_deref(), Some("sim-T02"));
        assert!(
            answer
                .message
                .contains("dependency validity and commit binding are unknown")
        );
    }

    #[test]
    fn a_result_is_no_longer_valid_once_a_depended_on_path_changed_after_it_finished() {
        let rows = vec![row(
            1,
            "sim-T02",
            "2026-10-04T10:00:00Z",
            Some("2026-10-04T10:02:00Z"),
            "builder check -p sim",
        )];
        let changed = vec![change("crates/sim/src/a.rs", "2026-10-04T10:03:00Z")];
        let ask = Ask {
            command: "builder check -p sim",
            wrappers: &[],
            execution: &slots(2),
        };
        let answer = decide(&ask, &rows, &changed);
        assert_eq!(answer.answer, "none");
    }

    #[test]
    fn transitive_builder_paths_and_changes_during_execution_invalidate_reuse() {
        let rows = vec![row(
            1,
            "ink-T7",
            "2026-10-04T10:00:00Z",
            Some("2026-10-04T10:02:00Z"),
            "builder build --bin gg",
        )];
        let ask = Ask {
            command: "builder build --bin gg",
            wrappers: &[],
            execution: &slots(2),
        };
        for path in [
            "crates/gg-core/src/host.rs",
            "crates/gg-sessions/src/lib.rs",
            "builder.lock",
        ] {
            for seen in [
                "2026-10-04T10:00:00Z",
                "2026-10-04T10:01:00Z",
                "2026-10-04T10:03:00Z",
            ] {
                assert_eq!(decide(&ask, &rows, &[change(path, seen)]).answer, "none");
            }
        }
        let answer = decide(&ask, &rows, &[]);
        assert_eq!(answer.answer, "unknown");
        assert!(
            answer
                .message
                .contains("dependency validity and commit binding are unknown")
        );
    }

    #[test]
    fn a_command_that_edits_first_is_never_answered_from_a_result() {
        let answer = ask_for(
            "sed -i 's/a/b/' a.rs && scripts/builder-slot builder check -p sim",
            &wrappers(),
        );
        assert_eq!(answer.answer, "none");
        assert!(answer.message.contains("edits files"));
    }

    #[test]
    fn a_command_with_no_build_or_test_in_it_is_not_matched() {
        let answer = ask_for("cat a.rs; sleep 3", &[]);
        assert_eq!(answer.answer, "none");
        assert!(answer.message.contains("nothing in this command"));
    }

    #[test]
    fn a_command_another_seat_is_running_is_running_while_a_slot_is_free() {
        let answer = ask_for("scripts/builder-slot builder test -p sim", &wrappers());
        assert_eq!(answer.answer, "running", "{answer:?}");
        assert_eq!(answer.seat.as_deref(), Some("sim-T03"));
    }

    #[test]
    fn a_slotted_command_past_the_slot_count_is_waiting_not_running() {
        let started = |id: i64, at: &str, command: &str| row(id, "sim-T0x", at, None, command);
        let rows = vec![
            started(
                1,
                "2026-10-04T10:00:00Z",
                "scripts/builder-slot builder test -p a",
            ),
            started(
                2,
                "2026-10-04T10:01:00Z",
                "scripts/builder-slot builder test -p b",
            ),
            started(
                3,
                "2026-10-04T10:02:00Z",
                "scripts/builder-slot builder test -p sim",
            ),
            started(4, "2026-10-04T10:03:00Z", "npm test"),
        ];
        let waiting = waiting_uses(&rows, &wrappers(), &slots(2));
        assert_eq!(waiting.into_iter().collect::<Vec<_>>(), vec![3]);
        let execution = slots(2);
        let wrappers = wrappers();
        let sim = Ask {
            command: "scripts/builder-slot builder test -p sim",
            wrappers: &wrappers,
            execution: &execution,
        };
        let answer = decide(&sim, &rows, &[]);
        assert_eq!(answer.answer, "waiting", "{answer:?}");
        assert!(answer.message.contains("this is inferred"));
        let first = Ask {
            command: "scripts/builder-slot builder test -p a",
            ..sim
        };
        assert_eq!(decide(&first, &rows, &[]).answer, "running");
    }

    #[test]
    fn a_stored_slot_count_is_ignored_whatever_its_value() -> anyhow::Result<()> {
        for stored in [json!(5), json!(0), json!(65), json!(-1), json!("two")] {
            let parsed = FleetDefinition::from_value(&json!({"build_slots": stored}));
            assert!(parsed.map_err(anyhow::Error::msg).is_ok());
        }
        Ok(())
    }

    #[test]
    fn a_third_unit_lets_the_third_queued_command_run() {
        let started = |id: i64, at: &str| {
            row(
                id,
                "sim-T0x",
                at,
                None,
                &format!("scripts/builder-slot builder test -p p{id}"),
            )
        };
        let rows = vec![
            started(1, "2026-10-04T10:00:00Z"),
            started(2, "2026-10-04T10:01:00Z"),
            started(3, "2026-10-04T10:02:00Z"),
            started(4, "2026-10-04T10:03:00Z"),
        ];
        let three = waiting_uses(&rows, &wrappers(), &slots(3));
        assert_eq!(three.into_iter().collect::<Vec<_>>(), vec![4]);
        assert_eq!(waiting_uses(&rows, &wrappers(), &slots(2)).len(), 2);
    }

    #[test]
    fn a_failed_result_that_still_holds_is_reported_as_failed() {
        let mut failed = row(
            1,
            "sim-T02",
            "2026-10-04T10:00:00Z",
            Some("2026-10-04T10:02:00Z"),
            "builder check -p sim",
        );
        failed.is_error = Some(true);
        let ask = Ask {
            command: "builder check -p sim",
            wrappers: &[],
            execution: &slots(2),
        };
        let answer = decide(&ask, &[failed], &[]);
        assert_eq!(answer.answer, "unknown");
        assert!(answer.message.contains("captured tool error"));
    }
    #[test]
    fn an_unconfirmed_background_result_is_unfinished_without_claiming_liveness() {
        let mut pending = row(1, "worker-1", "2026-10-04T10:00:00Z", None, "builder test");
        pending.is_error = None;
        pending.content = Some(serde_json::json!(
            "Command running in background with ID: pending"
        ));
        let ask = Ask {
            command: "builder test",
            wrappers: &[],
            execution: &slots(1),
        };
        let answer = decide(&ask, &[pending], &[]);
        assert_eq!(answer.answer, "unfinished");
        assert!(answer.finished_at.is_none());
        assert!(answer.is_error.is_none());
        assert!(answer.message.contains("current process state is unknown"));
    }
}
