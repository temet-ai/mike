---
title: Fleet events
status: active
created: 2026-10-21
updated: 2026-11-06
summary: The append-only record of a fleet run, `launch.fleet_events`: what a row is, every kind and its payload, and how a state is read from the rows.
---

# Fleet events

A fleet's run is written as one append-only record, the table `launch.fleet_events`.
A row is one thing that happened: a seat launched, a commit landed, a check finished,
a report filed. Rows are only ever added, never updated. Any state a fleet shows (a
task's seat, whether it is working or awaiting a verdict, whether the fleet is done)
is read by folding the rows of a run in `id` order, so there is no stored state to go
stale and a run can be replayed from its rows alone.

mike writes 22 of the 26 kinds. `plan_versioned`, `seat_heartbeat_stale`, `operator_touched`
and `resource_admitted` exist in the table and the code, and a problem's dossier reads
`plan_versioned` (`launch/fleet/leaves/solver.rs`), but nothing writes them. Writers, relative to `crates/mike/src/`:

| Kinds | Written by |
|---|---|
| `seat_launch_requested`, `seat_launch_failed` | `launch/fleet/seat.rs` when a seat is started; the supervisor (`supervise.rs`) also writes `seat_launch_failed` |
| `seat_launch_confirmed`, `seat_first_request` | `launch/fleet/supervise_live.rs` |
| `seat_process_gone`, `seat_woken`, `seat_closed` | the supervisor (`launch/fleet/supervise.rs`, `supervise_live.rs`); `cleanup.rs` also writes `seat_closed` when the run closes |
| `commit_landed` | `fleet_commit`, as the commit lands (`launch/fleet/commit_landed.rs`, called from `commit.rs`) |
| `check_queued`, `check_finished` | a configured check (`launch/fleet/check_events.rs`); a check left unfinished is settled with state `interrupted` |
| `tree_broken_by` | `launch/fleet/shared_tree.rs`, when the shared tree's break is attributed |
| `floor_passed`, `floor_failed` | `launch/fleet/floor.rs`, when a task's floor has run |
| `report_filed`, `verdict_recorded` | the fleet report and verdict handlers (`api/fleet/report.rs`, `verdict.rs`), after their note is written; `verdict_recorded` is also written per criterion by the report judge (`launch/fleet/leaves/report_judge.rs`, `judge.rs`) |
| `criterion_suspect` | `launch/fleet/propose_tick.rs` |
| `question_asked`, `question_answered` | `launch/fleet/questions.rs`; the answer handler (`api/questions/answer.rs`) appends `question_answered` only when the answer settles the question, so a decline or a refused answer writes none |
| `problem_opened`, `problem_decided`, `problem_escalated` | `launch/fleet/problems.rs`; a decision arrives through `POST /api/fleets/{fleet}/decide` |
| `stop_requested` | `launch/fleet/shutdown.rs`, when a stop moves a fleet to stopping |

Code: `launch/fleet/events.rs` (kinds and payloads), `store/persistence/fleet/events.rs`
and `events_sql/` (persistence), migration `20261022000000_launch_fleet_events.sql`.
Column reference: [`SCHEMA.md`](SCHEMA.md#launchfleet_events).

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
| `check_finished` | A check finished (`state` is `finished`, `cancelled` or `interrupted`). | `receipt_id`, `sha`, `passed`, `state`, `exit` |
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
| `problem_opened` | A problem matching a trigger was opened. | `problem_id`, `trigger`, `detail` |
| `problem_decided` | The problem was decided. | `problem_id`, `decided_by`, `decision`, `reason`, `evidence_read`, `minutes` |
| `problem_escalated` | The problem was passed up. | `problem_id`, `reason`, `question` |

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

The fold is `launch/fleet/fold.rs`; `status_tasks.rs` reads it. In outline:

| State | Derived as |
|---|---|
| A task's seat | The newest `seat_launch_confirmed` for the task whose session has no later `seat_closed` or `seat_process_gone`. |
| A task with no seat | No such event, or the newest is `seat_launch_failed`. |
| A task awaiting a verdict | A `report_filed` newer than the newest `verdict_recorded` for it. |
| The fleet stopping | A `stop_requested` with any seat still live. |
| The fleet done | A `stop_requested` and no seat live, or at least one seat confirmed, none live, and every task accepted. |

## Fixtures

`crates/mike/tests/fixtures/fleet-runs/<run>/` holds three measured runs as
`launches.json`, `notes.json`, `receipts.json` and `fleet.json`, with every text field
dropped. `scripts/fleet-run-export <run id>` writes them from the capture database,
read-only.
