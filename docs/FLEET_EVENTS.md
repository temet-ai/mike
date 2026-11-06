---
title: Fleet events
status: active
created: 2026-10-21
updated: 2026-10-22
summary: The append-only record of a fleet run, `launch.fleet_events`: what a row is, every kind and its payload, and how a state is read from the rows.
---

# Fleet events

A fleet's run is written as one append-only record, the table `launch.fleet_events`.
A row is one thing that happened: a seat launched, a commit landed, a check finished,
a report filed. Rows are only ever added, never updated. Any state a fleet shows (a
task's seat, whether it is working or awaiting a verdict, whether the fleet is done)
is read by folding the rows of a run in `id` order, so there is no stored state to go
stale and a run can be replayed from its rows alone.

Nothing in fleet 1 writes an event: the table, its kinds and the persistence exist,
and the three measured runs exist as fixtures under
`crates/mike/tests/fixtures/fleet-runs/`. The writers belong to fleet 2, and they
land one task at a time. Written so far (T12, `baaeb74b`): `report_filed` and one
`verdict_recorded` per criterion, appended by the fleet report and verdict handlers
(`crates/mike/src/api/fleet/report.rs`, `verdict.rs`) after their note is written;
`check_queued` and `check_finished` for a configured check (T11, `d0dca1f6`,
`crates/mike/src/launch/fleet/check_events.rs`); `tree_broken_by` when the shared tree's break is attributed (T15, `shared_tree.rs`); `seat_launch_failed`, `seat_process_gone`, `seat_woken` and `seat_closed` from the supervisor (T10, `fd7c7b51`, `d4139316`, `supervise.rs`); `floor_passed` and `floor_failed` when a task's floor has run (T13, `ba9bc34e`, `floor.rs`); `problem_opened`, `problem_decided` and `problem_escalated` from the problem machinery (T17, `da4ff83d`, `problems.rs`; a decision arrives through `POST /api/fleets/{fleet}/decide`, `27c69c4a`), and `verdict_recorded` also carries the judge's `needed` (`70a0610d`); `stop_requested` when a stop moves a fleet to stopping (T12, `b707df24`, `crates/mike/src/launch/fleet/shutdown.rs`); `question_asked` on an ask, appended through `crates/mike/src/launch/fleet/questions.rs`.
`question_answered` is built there (`answered_event`, `record_answered`) and appended by
the answer handler (`crates/mike/src/api/questions/answer.rs`, T12 `4aa6bbd3`) only
when the answer settles the question; a decline or a refused answer writes none. The
other kinds have no writer yet.

Design: `artifacts/2026-10-21/fleet-redesign/design.md` section 1.1. Code:
`crates/mike/src/launch/fleet/events.rs` (kinds and payloads),
`crates/mike/src/store/persistence/fleet/events.rs` and `events_sql/` (persistence),
migration `20261022000000_launch_fleet_events.sql`. Column reference:
[`SCHEMA.md`](SCHEMA.md#launchfleet_events).

## A row

| Column | What it holds |
|---|---|
| `id` | Identity; the order of the run. |
| `run_id` | The run the event belongs to (`launch.runs`, deleted with it). |
| `task` | The task the event is about, NULL for an event about the whole run. |
| `session_uuid` | The seat session the event is about, NULL when it is about none. |
| `kind` | One of the kinds below; the table refuses any other. |
| `payload` | The kind's fields: ids and shas, never captured text (no brief, body, output or message). |
| `occurred_at` | When it happened. |

## Persistence

| Call | Returns |
|---|---|
| `append_fleet_event(&NewFleetEvent)` | Adds one event and returns its id. |
| `fleet_events_since(run, after_id)` | The run's events after an id, in order; a reader that remembers the last id it saw reads only what is new. |
| `fleet_events_for_task(run, task)` | The run's events about one task, in order. |

All three are methods of `PersistHandle`.

## Kinds

All 26 kinds in `EventKind` (`events.rs`), grouped by what it is about. The payload
column lists the fields of that kind's struct.

### The plan

| Kind | Written when | Payload |
|---|---|---|
| `plan_versioned` | The definition's plan changes (criteria, tasks, files, operations). | `version`, `brief_version` |

### Seats

| Kind | Written when | Payload |
|---|---|---|
| `seat_launch_requested` | mike asks for a seat to be started for a task. | `launch_id` |
| `seat_launch_confirmed` | The seat's session registered. | `launch_id` |
| `seat_launch_failed` | The start failed; the task has no seat. | `launch_id`, `reason` |
| `seat_first_request` | The seat made its first request. | `request_id` |
| `seat_heartbeat_stale` | The seat's lease lapsed. | `silent_seconds` |
| `seat_process_gone` | The seat's process is no longer there. | `launch_id` |
| `seat_closed` | The seat was closed. | `reason` |
| `seat_woken` | A seat that had stopped was woken. | `via` |

### Commits and checks

| Kind | Written when | Payload |
|---|---|---|
| `commit_landed` | A seat's commit reached the branch, written by `fleet_commit` as the commit lands (`commit.rs`). | `sha` |
| `tree_broken_by` | A check failed and the commit that broke the tree is known. | `sha`, `paths` |
| `check_queued` | A check was queued. | `receipt_id`, `operation` |
| `check_finished` | A check finished. | `receipt_id`, `sha`, `passed` |
| `floor_passed` | A task's computed floor (its `done_when` checks) passed. | `sha` |
| `floor_failed` | A task's floor failed. | `sha`, `check` |

### Reports and acceptance

| Kind | Written when | Payload |
|---|---|---|
| `report_filed` | A worker filed a slice report. | `note_uuid` |
| `verdict_recorded` | A verdict on a report. The acceptance seat's carries its session and is read through its decision note; the judge leaf's (`leaves/report_judge.rs`) carries no session and is the task's acceptance by itself (`acceptance.rs`). `note_uuid` is the report judged, `needed` says what a fail wants. | `note_uuid`, `criterion`, `passed`, `needed` |
| `criterion_suspect` | A criterion's check is doubted after repeated failures. | `criterion`, `failures` |

### Questions and problems

| Kind | Written when | Payload |
|---|---|---|
| `question_asked` | A seat asked a question. | `question_id` |
| `question_answered` | The question was answered. | `question_id` |
| `problem_opened` | A problem matching a design trigger was opened. | `problem_id`, `trigger` |
| `problem_decided` | The problem was decided. | `problem_id`, `decided_by` |
| `problem_escalated` | The problem was passed up. | `problem_id` |

### Declared resources

| Kind | Written when | Payload |
|---|---|---|
| `resource_admitted` | A declared heavy operation was given a unit of a resource after waiting for it. | `queued_at`, `admitted_at`, `resource`, `units` |

### The run

| Kind | Written when | Payload |
|---|---|---|
| `stop_requested` | A stop was requested. | `by` |
| `operator_touched` | The operator changed something by hand. | `what` |

## Reading a state

The fold extends `launch/fleet/status_tasks.rs`. In outline:

| State | Derived as |
|---|---|
| A task's seat | The newest `seat_launch_confirmed` for the task whose session has no later `seat_closed` or `seat_process_gone`. |
| A task with no seat | No such event, or the newest is `seat_launch_failed`. |
| A task awaiting a verdict | A `report_filed` newer than the newest `verdict_recorded` for it. |
| The fleet stopping | A `stop_requested` with any seat still live. |
| The fleet done | A `stop_requested` and no seat live, or no seat live and every task accepted. |

## Fixtures

`crates/mike/tests/fixtures/fleet-runs/<run>/` holds the three measured runs
(`fleet-radar-retire-ratatui-db99de21`, `fleet-writing-room-0656d7b7`,
`fleet-cms-ux-8d1a15dc`) as `launches.json`, `notes.json`, `receipts.json` and
`fleet.json`, with every text field dropped. `scripts/fleet-run-export <run id>`
writes them from the capture database, read-only.
