# Watch a fleet and step in

A running fleet needs you only when a session asks for something it cannot get elsewhere, or
when mike cannot fix a fault by itself. This page covers the status screen, how to answer a
session, what mike repairs on its own, and how to stop a fleet.

## The commands

| Command | Shows |
|---|---|
| `mike fleet list [--project <path>]` | Every fleet, newest first: slug, state, items missing, when it was created |
| `mike fleet status <fleet>` | One screen: what is missing, every task, every open session, the newest verdicts, what is blocked |
| `mike fleet status <fleet> --watch 5` | The same, redrawn every 5 seconds |
| `mike fleet summary` | One line for a status bar: the newest running fleet's progress |
| `mike fleet focus <fleet> [seat]` | Brings a session's pane to the front: the one you name, else the first blocked one, else the one with the newest failure. Add `--switch` to change workspace. |
| `mike fleet record <fleet>` | For a finished run: time to done, minutes lost to faults, tokens per accepted task. `--against <fleet>` compares two runs. |
| `mike fleet stop <fleet>` | Closes the fleet's sessions and ends the run |

`<fleet>` is the fleet's slug or its id. Add `--json` to `list`, `status` and `record` for
scripts. Every command takes `--config <FILE>`. The full list is in
[Command line](../command-line.md).

## The status screen

![The mike fleet status screen for the example fleet: T1 accepted, T2 working, T3 waiting for T1 and T2, and one NEEDS YOU line](../images/fleet-status.png)

The screen lists every task with its state and the session working on it, every open session
with how much of its context it has used, and the newest verdicts. A fleet is in one of these
states: `draft`, `ready`, `starting`, `running`, `stopping`, `failed` or `done`.

Three flags appear straight under the header when they apply:

- NEEDS YOU lists what a session has asked of you, with its name.
- NOTHING IS MOVING means every open session has been idle for five minutes or more.
- PROBLEM shows something mike is working through, and whether it was passed to you.

Inside the fleet's own sessions, the [fleet-comms mod](../mods.md#fleet-comms) shows the same
state and the messages between sessions.

## Answer a session

A session never stops to ask you a question in its own pane, because the fleet would wait behind
it. It writes a note for you, titled with what it needs, and carries on with whatever else it
can do. The note appears under NEEDS YOU. To answer:

1. Run `mike fleet focus <fleet>` to bring that session's pane to the front.
2. Type your answer in the pane, as you would in any Claude Code session.

The session marks the note answered when it is settled.

Questions between sessions do not reach you. A worker that needs something from another session
asks it directly. A question from a blocked worker that is still open after 5 minutes goes to
the reference session.

## The Fleet page in the browser

Open **Fleet** in the menu, or <http://localhost:9990/fleet>. It shows the sessions of one
project, chosen at the top, over the last 24 hours, in three views: **Relationships** (a graph
of sessions linked by the messages between them), **Board** and **Notebook**. It lists any
captured sessions, fleet or not. It does not show a fleet's tasks or verdicts; use
`mike fleet status` for those. See [The web UI](../web-ui.md) and [Projects](../projects.md).

![The Fleet page's Relationships view, with sessions linked by the messages between them](../images/fleet.png)

If there is nothing to show, the page says so:

- No projects captured yet: run Claude Code through mike and send a prompt.
- No sessions in this project's fleet: no session activity or messages for that project in the
  window.
- No active fleet activity in this window: the Board has only old sessions with no messages.
  **See all sessions** opens [Sessions](../reading-sessions.md).

## What mike repairs on its own

Every 20 seconds mike looks at each session in the fleet and does what you would otherwise do by
hand:

![mike's supervisor loop: a silent worker with unread mail is woken, then replaced; a worker that never starts is relaunched twice, then shown under NEEDS YOU; a problem goes to the problem-solver, at most three times before it comes to you](../images/fleet-recovery.svg)

| What happens | What mike does |
|---|---|
| A worker sends nothing within 90 seconds of being opened | Closes it and opens it again, up to twice; then shows it under NEEDS YOU |
| A worker's process has ended | Records it closed and opens a new worker for the task |
| A worker has unread mail and has been silent for 5 minutes | Wakes it over Remote Control; if it is still silent 2 minutes later, replaces it |
| A check runs past its time limit | Stops it and queues it again once; a second time, it counts as a failure for the task |
| The reference session passes 600,000 tokens of context | Opens a new reference session to take over |
| A session will not close when the fleet stops | Tries three times, then records it closed and shows it under NEEDS YOU |

## Problems mike decides

Some faults need a decision rather than a restart. mike opens a problem for them and hands it to
the problem-solver, a model call that reads what happened and picks one action. A problem is
opened when, for example:

- the gate fails on a clean copy before any worker opens;
- a check fails in files no task owns;
- a worker disputes a fail, or asks a question nobody answers;
- an outcome fails three times;
- a session stalls, or every session is idle;
- something in the machine, not the work, is at fault.

The decision is one of: record the fault and carry on, message the workers, change the
definition (a brief, a task's files or order, a rule), add a one-file task, waive an obligation,
correct a task's `done_when`, accept other proof for a check that cannot run during the run,
drop a stuck check result, run a narrower check again, change a setting, or ask you.

A problem gets at most three decisions, and the third goes to you whatever it says. A run hands
at most 20 problems to the problem-solver. When the problem-solver asks you, the question says
what is needed, why only you can answer it, and what each answer would set in motion.

## Check a surprising gate verdict

A `PRE-EXISTING` mail is not proof that the failure predates this run. In the older source
checked at `dfb064e2`, the gate searches a bounded commit history (up to 200 commits)
and keeps only commits whose `Fleet-Seat` trailer exactly matches a task's current
`seat (session)` pair in the definition. No matching commits can produce that mail;
matching commits for which no suspect is identified produce a separate no-match verdict.
A missing trailer or a changed seat/session mapping could therefore prevent attribution.

From source commit `3e98c7e0` (also present in `0ad4acd5`), matching uses the seat
name before ` (` in the trailer, not the session UUID. A respawned session can now
match the same task seat. The mail still says `PRE-EXISTING`, but its explanation
now limits the claim to an inability to tie the inspected commits to a run seat;
it does not establish that the failure existed before the run. The bounded history
and need to verify the failing SHA still apply.

Before adding a repair task:

1. Save the gate command, failing SHA, output/log reference, run id and last recorded
   passing SHA, if there is one. Do not substitute the checkout's current HEAD for
   the SHA that failed.
2. Read the full definition with `get_fleet` and compare its task seat/session pairs
   with the failing commit's trailer (seat name on newer source, the full pair on the
   older source above). In that fleet's repository, replace the example
   SHA below with the one from the gate:

   ```sh
   git show -s --format=fuller <failing-sha>
   git show -s --format='%(trailers:key=Fleet-Seat,valueonly)' <failing-sha>
   ```

3. Check whether the owning seat is already repairing it, and whether a later commit
   has a passing gate result. A suspect is evidence to investigate, not proof of cause.
   Coordinate with that owner before creating duplicate work; do not waive the failure
   merely because the mail labels it pre-existing.

[Issue #421](https://github.com/temet-ai/mike/issues/421) reports a run's own commits
being classified as pre-existing. It is now closed; the
reported incident was not independently reproduced. These source changes are not
proof that your installed release contains them or that the reported run recovered.

## A task stays queued with no listed blocker

A task's digest `ready` flag is not a launch-permission flag. In source `dfb064e2`, it
comes from acceptance/readiness evidence or passed exports. Dependency eligibility is
computed separately from `after`. Fleet-level `ready` is different again: it says the
definition has no missing items. Do not infer a scheduler cause from `ready: false`
alone, or assume an empty blockers list proves there is capacity to launch.

1. Save `mike fleet status <fleet> --json`, the full `get_fleet` definition and its
   `view: "digest"` response, with timestamps, run id, machine and installed version.
   Keep these locally; redact private briefs, paths and output before sharing.
2. Compare each queued task's `after` list with the predecessors' current acceptance
   evidence and obligations, not just a worker's completion message or a pass count.
3. Compare assigned and open seats, queued launches and available capacity. Check pane
   limits such as `launch_max_surfaces` as well as any reported seat limit and memory
   admission reason. An accepted task and a closed process are different facts.
4. Read status again after a legitimate acceptance or seat-close event. Preserve both
   snapshots if the task remains queued without an explanation, including any
   `not started` reason and relevant logs. Ask the coordinator to investigate the
   recorded state rather than launching a second worker blindly.

[Issue #422](https://github.com/temet-ai/mike/issues/422), now closed, reports
satisfied dependencies but no launch. Its suggested missed-retry cause is a hypothesis,
not a verified diagnosis here. Removing `after` reportedly started the tasks, but it
also changes the dependency contract: **do not remove dependencies, clear seat/session
fields or restart the fleet as a generic repair**. Preserve the original definition
and evidence first; change the plan only when that change is actually intended.

### Newer source: release evidence and queued reasons

Source `0ad4acd5` adds `queued: <reason>` to status for recorded staffing holds:
producers not released (named in the reason), an existing launch claim, the start
baseline gate, host memory, a warm sibling, or no room. Read the specific reason
before changing the definition. These reasons are in-memory observations from the
staffing tick, not a durable history or a guarantee that every delay has a reason.
An annotated queued task remains eligible for staffing on subsequent ticks.

The producer-release check now filters evidence by the definition's **`check.command`**,
not every gate result at that SHA. Command strings are compared after trimming;
legacy events without a command remain eligible. Thus a different `gate.command`
failing at the same SHA should not be read as the configured check failing. This
separates dependency release from the final gate; it does not waive the gate failure.
An accepted/ready producer is held by a recorded failed check, while a producer that
has only committed needs a passed check. Accepted/ready producers may release without
check evidence; that is not proof that a check passed.
Check the producer SHA and any containing-commit evidence, not just the latest result
from an unrelated command.

From `4a2ef93e`, the fleet tick also attempts to close an open seat whose task is
ready and not in rework, and records a close event on success. A failed stop or failed
close-event write is logged for another tick to retry. Staffing and closure are
separate steps: do not promise immediate launch, and verify pane/process and recorded
seat state rather than equating acceptance with process exit. Re-read the queue after
closure instead of deleting `after` dependencies.

These are post-alpha.5 source changes checked through `0ad4acd5`, not an assertion
that a release or running installation has them. The incident in #422 was not
independently reproduced or verified as recovered here.

## Stop a fleet

- `mike fleet stop <fleet>` asks the fleet to shut down. It closes each open session's pane and
  prints, per session, whether the pane closed and whether its process has exited. The fleet
  reads `stopping` until they are gone, then `done`. A stopping fleet's definition cannot be
  edited.
- A failed start puts the fleet in `failed`. Ask Claude for its state (`get_fleet`, see
  [MCP](../mcp.md)) and read the start report, fix the definition and mark it ready again.
- A red baseline, where the gate fails before any session opens, lets only the named repair
  task run.
- At the pane limit no new pane opens, and a task whose session could not start is shown as not
  started with the reason. Close a finished pane or raise `launch_max_surfaces`.

To end one session only, run `mike spawn kill --session <id>`.

Every `mike fleet` command talks to the running mike. With mike stopped, `mike fleet list` exits
with `could not reach the inspect API`; start mike first (see [Setup](../setup.md)).
