# After a run

When a fleet closes, its commits are on your branch and pushed. mike also writes a record of how
the run went, a retrospective session reads the run for anything worth fixing, and what the
workers learned can be offered to the next fleet. This page covers all three.

## The record of the run

mike writes one record per fleet when the run closes, measured from what it captured, not
written by a model. Print it with:

```sh
mike fleet record site-docs
```

It leads with four numbers:

| Number | What it measures |
|---|---|
| Time to done | From the start to the last task accepted |
| Minutes lost to faults | Time with a task and no session, relaunch gaps, reports waiting for a verdict, commands waiting for a resource, checks past their limit, and stopping |
| Tokens per accepted task | The tokens all sessions spent, divided by the tasks accepted |
| Manual interventions | Your turns typed into a fleet's sessions, sessions started by hand, and notes for you that were answered |

Below them, per session, it shows the minutes working and waiting, requests and tool calls, the
time to its first commit, and the tools it was offered against the tools it used.

To compare two runs, for example after changing the definition or a project's agents:

```sh
mike fleet record site-docs-2 --against site-docs
```

`--recompute` measures a run again and replaces its stored record. `--json` prints it for
scripts.

## The retrospective

After the close, mike starts one more session, the retrospective, which reads the whole run: the
sessions' turns, the checks, the verdicts and the problems. It writes what it finds as notes and
changes nothing: it does not edit files, run commands, send messages or open issues. It runs on
a small model by default.

Turn it off with `retrospective.enabled: false` in the definition. To let it change files, the
definition has to say so explicitly, with what it may change and a separate branch to change it
on.

## Lessons for the next fleet

A worker that learns something the next run should know from the start, such as a build that
cannot run twice at once or a folder nobody should edit, can write it as a note marked to carry
over. mike offers it to you as a lesson in the approval queue (see
[Memory](../memory.md#keep-a-correction-as-a-lesson)), scoped to the project and the kind of
work. A second note about the same thing adds to the first instead of making another.

Once you approve it, it is added to the reading of that kind of work's template in later fleets,
so every worker copied from it starts with it. Nothing is added without your approval.

## Before the next fleet

- Read the record's minutes lost to faults first. Relaunch gaps point at the terminal or the
  machine; reports waiting for a verdict point at the judge's model and key; commands waiting
  for a resource point at `.claude/fleet/operations.json`.
- Read the retrospective's notes for briefs that were unclear, files that two tasks needed, and
  checks that could not run.
- When workers kept reading the same large documents, list them in
  `.claude/fleet/templates/<kind of work>.md`, so the template reads them once for every worker.
- Correct `.claude/fleet/project.md` if a session got the build or the check wrong.
