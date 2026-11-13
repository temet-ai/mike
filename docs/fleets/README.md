# Fleets

A fleet is one goal split into tasks, each given to its own Claude Code session that mike
opens, all committing to one branch of one checkout. mike starts the sessions, checks what
they commit, judges their reports against the definition and closes the run. The code is
`crates/mike-features/src/launch/fleet/`; the files a fleet's sessions read are under `.claude/fleet/`.

## For people using fleets

The public user guide, published on <https://temet.ai/docs/mike/fleets/>:

- [Fleets: several sessions on one goal](../user-guide/fleets/README.md): the overview, the
  example and what a fleet needs.
- [Run your first fleet](../user-guide/fleets/first-fleet.md),
  [Write a fleet definition](../user-guide/fleets/definition.md),
  [How a fleet runs](../user-guide/fleets/how-a-fleet-runs.md),
  [Share one checkout](../user-guide/fleets/shared-checkout.md),
  [How work is accepted](../user-guide/fleets/acceptance.md),
  [Watch a fleet and step in](../user-guide/fleets/watch-and-recover.md) and
  [After a run](../user-guide/fleets/after-a-run.md).

The diagrams on those pages are animated SVGs in `docs/user-guide/images/fleet-*.svg`.

## For people changing mike

| Document | What it holds |
|---|---|
| [`LAUNCH_DOMAIN.md` section 5](../LAUNCH_DOMAIN.md#5-fleets) | The mechanism: the `mike fleet` commands and API, how a fleet starts and staffs itself, declared operations, the check and the gate, commits, reports and verdicts, the tick and its steps, the judge, checker and problem-solver, questions, goals, the run record and the reference fleets. |
| [`events.md`](events.md) | The append-only record of a run, `launch.fleet_events`: every event kind, its payload and writer, and how a shown state is folded from the rows. |
| [`isolated-checks.md`](isolated-checks.md) | How the check and the gate grade an exact commit in the managed worktrees under `.claude/worktrees/`, and where a working-tree command differs. |
| [`../animations/fleet-run.html`](../animations/fleet-run.html) | An interactive walkthrough of a run. It shows the coordinator, acceptance and observer seats of an earlier design; a fleet now opens none of them. |

## What to put in `gate`

A definition's `gate` has two commands:

- `gate.command` is the **quick** gate: format and compile, nothing slower (in this repository
  `make gate-quick`). mike runs it once on a clean copy of the branch before any seat opens, and
  again on every pushed commit, so a slow command here holds every seat at start and queues a
  long run per push.
- `gate.full_command` is the **full** gate (in this repository `scripts/gate --json check`). mike
  runs it once when the work is done: when every seat has exited, at the branch's newest commit,
  and when the acceptance seat asks with `mike fleet gate`. The fleet stays open until it passes.
  Left empty, `gate.command` is used in its place.

`define_fleet` answers with a warning under `at_ready.missing` when `gate.command` looks like a
full gate and `gate.full_command` is empty: its declared operation's name or purpose says
"full", the operation allows more than ten minutes, or a recorded run took more than ten
minutes. The code is `launch/fleet/gate_full.rs`.

The files a running fleet reads, under `.claude/fleet/`:

| File | What it is |
|---|---|
| `rules.md` | The rules every session of a fleet works under. |
| `fleet-command.md` | The `/fleet` command that writes a definition. |
| `leaves/judge.md`, `checker.md`, `problem-solver.md` | The prompts of the three model calls. |
| `templates/<discipline>.md` | What a template reads, per kind of work. |
| `project.md`, `operations.json` | How this repository is built and checked, and its declared operations. |
| `observer-retro.md` | The checklist the retrospective follows after a run. |

mike compiles its own copies of the rules, the leaf prompts, the `/fleet` command and the
`fleet-worker` skill into the binary (`launch/fleet/bundle.rs`); a project's own file of the
same name takes precedence.
