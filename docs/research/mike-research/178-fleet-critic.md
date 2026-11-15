---
title: "#178 fleet critic: what mike answers today, options, recommendation, measure"
status: active
created: 2026-11-15
updated: 2026-11-15
summary: mike already answers part of "done before" and "useful"; nothing answers "actionable" before a check is queued, and nothing asks "smarter way". Recommend mechanical answers in fleet_check first, then a launched observer critic reading them; measure machinery share by all tokens per seat and checks queued on a known-red tree.
session_id:
  - ccbfdfff-7eae-42cd-b657-bd1e714474bb
---

## What is true now

Code at `main` ef99f7e16, data from the capture DB, read 2026-11-15.

**Which of the four questions mike already answers**

| Question | Answered today | Where |
|---|---|---|
| 1. Done before? | Partly. The fleet's own check runs once per tree state, and a later asker is added to the same receipt. A gate rerun at the same state reuses its verdict. A commit of unchanged paths is refused. The next definition sees the previous run's findings. | `crates/mike-fleet/src/launch/fleet/commit_verify.rs:69`, `store/persistence/fleet/checks.rs:204` (`add_tree_check_asker`), `launch/fleet/gate_rerun.rs:82`, `launch/fleet/commit.rs:394`, `api/fleet/at_ready.rs:114` |
| | Not for declared operations: reuse is `disabled` unless the operation declares a `cache_key`, which none in this run does. The reuse lookup (`/api/fleets/{fleet}/reuse`) exists but is manifest-exempt and only asked for timed commands. | `launch/fleet/operation.rs:166-170`, `api/manifest/fleets.rs:66`, `launch/fleet/reuse.rs:158-187` |
| 2. Useful? | Partly. A report whose paths no check covers says so. `at_ready.capacity` gives seats, free memory and queue minutes before the start. | `launch/fleet/report_commit.rs:7`, `api/fleet/observations.rs:25` |
| 3. Actionable? | No. `fleet_check` queues a check without saying the latest check at HEAD failed. That fact appears only in `what_changed` view `standing`. | `api/fleet/check.rs:15-62`, `api/changes/standing_view.rs:36` |
| 4. Smarter way? | No component asks. The observer seat is never launched: `ControlSeats` sets `observer: None` in both constructors. Its role forbids steering and lets it mail only a coordinator. | `launch/fleet/plan.rs:46,54`; `~/.claude/agents/fleet-observer.md:22,24` |

**The digest** (`launch/fleet/tell_digest.rs:266-285`) carries task rows (state, idle since, passes and fails), builds waiting for a slot, and "every seat idle". It has no machinery share, no repeated results and no results left unacted on.

**The per-request effort measure already exists**, but only in the run record. `launch.request_efforts` holds one row per seat request with its trigger (operator, hook, mail, wakeup, tool_result, steering) and a label (work, machinery or waiting). Hooks are labelled by rule and the rest by Jev every 15 s, in batches of 8 (`launch/fleet/decisions/effort.rs:20-22,310`, `tick.rs:143`). The record's `effort_split` (`run_record.rs:242`) weights by `input_tokens + output_tokens` only (`store/persistence/fleet/effort.rs:211`), which leaves out cache reads, and cache reads are most of a seat's cost. This run, at the time of reading:

| Label | Requests | input+output tokens | Cache read + write tokens |
|---|---:|---:|---:|
| machinery | 38 | 31,160 | 1,093,983 |
| work | 61 | 29,741 | 5,849,720 |
| waiting | 4 | 12 | 153,108 |
| unlabelled | 88 | 53,516 | 10,280,208 |

Machinery is 51% of the labelled requests by the recorded weighting, but 16% once cache tokens count. The measure the issue asks for is built, but it is weighted wrong and absent from the digest.

**The open-issues waste in numbers** (run `fleet-open-issues-0c16da5b`, `launch.tree_checks`):
- 135 receipts: 8 check, 60 baseline, 12 command, 55 gate. Only 6 had a second asker join an existing receipt. 53 have no exit code.
- While the branch could not compile (2026-11-11 19:22–21:16 UTC): 65 non-gate receipts started, of which 4 passed, 18 failed and 43 have no exit code. They waited 2,016 minutes in the queue in total. 37 gates started in the same window, and 36 of them failed.
- fleet-runs entry 2: 959 minutes waited against 200 compiled, and 33 of 50 cargo runs failed. 36 of 48 judge FAILs asked for evidence the queue could not produce. T18 ran 14 `fleet_check` calls and 4 compiles for one Markdown edit. The change of method came from the operator at 04:15, 9.5 h in.

## Options compared

| | (a) Mechanical answers in fleet_check and staffing | (b) Observer seat with authority from the definition | (c) A periodic Jev or model read of the digest |
|---|---|---|---|
| What it is | Before queueing, `fleet_check` answers: "the latest check at HEAD failed at state X; this one will fail until <path> lands", "state X has receipt N", "your task's files are already committed". Staffing answers "a new seat for task T would duplicate seat S". | Launch the observer (`plan.rs:46,54`). Rewrite its role around the four questions. It tells the author (`created_by`) when there is no coordinator, and the definition decides whether it may change method itself. | A tick step beside `effort::tick` gives Jev the digest, the effort split and the tree-check facts as a Choice: continue, or one of a menu of method changes. |
| Build cost | About 3 changes in `mike-fleet`: `check.rs`, `reuse.rs` (declared operations), `staff.rs`. No new seat, no model. | Launch wiring, a role rewrite, and instruments to read: needs (a)'s facts and the effort split in the digest. | A new decision point (`decisions/config.rs:25`), its questions and a menu. |
| Running cost | 0 tokens. One DB read per `fleet_check`. | One opus seat for the run. Estimate for a 12 h run at 10-minute cycles: 72 cycles of about 10 requests at about 150k context, so about 108M cache-read tokens, about 4% of open-issues' 2.79B. It carries its own hook and mail machinery too. | Jev at $0.042 per million input tokens. The 680-step replay cost $0.0204 (fleet-runs entry 6), so pennies per run. |
| open-issues waste it catches | The red window: 65 checks (2,016 queue-minutes) and 36 failed gates answered "will fail" before queueing. The 129 receipts with one asker each get a "state X has receipt N" answer where the state repeats. T18's 14 checks on a Markdown edit get "no check covers these paths". Q1 and Q3 only. | Q2 and Q4: "finish, commit, build once" (04:15), sizing seats to one build permit, one shared scratch database. Only if it notices, and only as fast as its cycle. | Q1–Q3 signals and the menu options it is given. It cannot invent the 04:15 alternative unless the menu already holds it. Jev's confidence was below 0.7 on 154 of 680 steps. |
| What it leaves | No judgement: it cannot propose a different method. | Without (a) it re-derives facts from tool output every cycle, at model cost. | Not a critic. A classifier over a fixed menu. |

## Recommendation

Build (a) first, then (b) on top of it, and keep Jev as the labeller, not the critic.

- (a) catches the largest measured waste at no running cost. Most of open-issues' lost time was checks whose result was already known: 2,016 queue-minutes and 36 failed gates on a tree known to be red. It is also the fact layer any critic needs.
  1. `fleet_check` says before queueing when the latest check at HEAD failed, and at which state.
  2. Declared operations get the receipt-at-this-state answer (`reuse::ask` already computes it, `reuse.rs:158`).
  3. Staffing says when a task's files are already committed.
- Then launch the observer as the critic (`plan.rs:46,54`, role rewritten around the four questions, as the second comment on this issue says). Its instruments are (a)'s answers plus the effort split, moved into the digest (`tell_digest.rs:266`) and weighted by all tokens (`effort.rs:211`). Default authority is to propose once to `created_by`, with the alternative and its numbers. The definition can grant "change method".
- (c) is not chosen. It cannot propose an alternative it was not handed, and that alternative is the gap the operator filled at 04:15.

## Measure

Read from `launch.tree_checks` and `launch.request_efforts` per run, in the run record:

1. **Checks queued on a known-red tree**: receipts started while the latest check at HEAD had failed, and their queue-minutes. open-issues: 65 receipts and 2,016 minutes, plus 36 failed gates. Target 0 after (a), except checks a seat runs knowingly to reproduce the failure.
2. **Repeated-result checks per run**: receipts at a state that already had a finished receipt for the same command. Target 0 without a stated reason.
3. **Machinery share per seat, weighted by all tokens** (input, output, cache read and cache write). This run shows 16% by all tokens against 51% by input+output. Fix the weighting, show it in the digest, and expect it to fall run over run.
4. **The critic's own record**: suggestions made, how many were acted on, and whether the measure each one named moved. Kept in the next run's record and in fleet-runs.
