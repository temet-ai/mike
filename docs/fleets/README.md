# Fleets

A fleet is one goal split into tasks, each given to its own Claude Code session that mike
opens, all committing to one branch of one checkout. mike starts the sessions, checks what
they commit, judges their reports against the definition and closes the run. The code is
`crates/mike/src/launch/fleet/`; the files a fleet's sessions read are under `.claude/fleet/`.

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
