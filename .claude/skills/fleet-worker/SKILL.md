---
name: fleet-worker
description: Work as a worker or template seat of a Mike fleet: finish one task, commit with fleet_commit and report with fleet_report.
---

# Working in a fleet

**Template:** read only the brief's listed files, in order. Do not edit, build, test or summarize. End with `READY`; workers inherit this context.

**Worker:** before your first slice, read the named `.claude/fleet/project.md`; your task's obligations are notes tagged `fleet:<fleet id>`. Without a project agent, read one or two neighbouring files for conventions.

## Working

- Change only your task's files and generated outputs. Never accept or restore peers' snapshots.
- Use existing libraries, conventions and project commands; `ask` before adding a dependency, tool or convention.
- Never verify with update or force-pass flags.
- Send an outside failure's command and output to its owner. Do not repair peers' work.

## Reporting

Commit with `fleet_commit`, then report with `fleet_report` when your task's work is in, giving:

- `sha` (or `commit`): the commit you report, and `criteria`: the criterion ids the slice serves;
- `exercised` {command, exit, output_ref}: the command that exercised the changed behaviour, its exit code and where its output is kept; compiling is not proof that it runs;
- `tests_changed`: the paths of every test, assertion, fixture or baseline you changed, or `"none"`;
- `unestablished`: what you could not establish, linked to its acceptance obligation; `[]` when nothing;
- `slice`, `look_at` and `evidence` paths, and the commands you ran with their verdicts in `summary`, without rerunning them for the handoff.

Address a received FAIL before new work, then end the turn.

## Blocked work

A `blocked: true` ask says what you tried. End only when nothing can move; mail wakes you.
