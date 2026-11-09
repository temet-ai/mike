# How a fleet runs

This page follows a fleet from the moment it is marked ready until it closes: which sessions
mike opens and what each one does, how the sessions share one checkout without getting in each
other's way, how a task's work is checked and accepted, and what mike does on its own when a
session stalls or a check fails. To define, start, watch or stop a fleet, see
[Run several sessions on one goal](README.md).

The example continues from that page. In `~/projects/site` you asked for the install page and
the quickstart to be rewritten, Homebrew first, with a link check. The definition has three
tasks: T1 rewrites `install.md`, T2 rewrites `quickstart.md`, and T3 adds the link check, which
must wait for the other two (`after: T1, T2`).

## The sessions in a fleet

A fleet is a set of ordinary Claude Code sessions that mike opens with `mike spawn` (see
[Sessions](../sessions.md)), each told what part it plays. Most of the coordinating is done
by mike itself, in a loop that runs every 20 seconds, and by three short model calls. No
session is in charge of the others.

| Session | How many | What it does |
|---|---|---|
| The author | One, before the run | The Claude Code session where you ran `/fleet`. It writes the definition, marks it ready, tells you where to watch, and stops. |
| A template | One per kind of work | Reads the fleet's rules, the reading list for its kind of work and the definition, then ends its turn. mike records it and closes its pane. It never works on a task. |
| A worker | One per task | Copied from its template's conversation, so it starts with that reading already done, plus its own brief. It changes only its task's files, commits them, and reports. |
| The reference session | One | Answers the other sessions' questions about the code with a file and a line. It never edits, builds or decides. |
| The retrospective | One, after the run | Reads how the run went and writes findings as notes. It changes no file unless the definition allows it. |

A worker runs as the project's own specialist for its area when the definition names one (for
example a `frontend-developer` agent), with mike's `fleet-worker` skill added. Every session in
a fleet can read the same rules, and where a message from another session disagrees with those
rules, the rules win.

In the example, T1 and T2 are both `docs` work, so there is one `docs` template, and T1's and
T2's workers are both copied from it.

## From ready to the first commit

When the fleet is marked ready (or you run `mike fleet start`), mike does these things in order:

1. If the definition has a gate command (`gate.command`, the quick check mike runs on every
   pushed commit), mike runs it once on a clean copy of the branch. When that fails, no worker
   opens except a repair task the definition names, until the check passes.
2. It opens the status pane, the terminal pane that `mike fleet status --watch` draws.
3. It opens one template per kind of work and waits for each to finish reading. A template
   that has not finished after 8 minutes is dropped, and the workers of that kind start without
   the reading done in advance.
4. It opens the reference session.
5. Every 20 seconds it starts a worker for each task that has none and whose waiting-for tasks
   have passed. T1 and T2 start at once. T3 starts within 20 seconds of the second of them
   passing.

No more than 16 panes are open at once by default (`launch_max_surfaces` in
[Settings](../settings.md)). A task that has no room waits for the next round.

Each worker is started with a goal, a sentence saying when its task is done, taken from the
definition. The [fleet-comms mod](../mods.md#fleet-comms) in the worker's session hands it to
Claude Code's `/goal`, which keeps the session working until the goal holds. After 20 automatic
continuations the goal is cleared without accepting the task, so a worker that cannot finish
stops instead of running on.

A worker does not open your browser: mike blocks `open`, `xdg-open` and similar commands in the
sessions it starts, and the worker hands you a link instead. Set
`seat_settings.env.MIKE_FLEET_BROWSER_GUARD` to `"0"` in the definition to allow it.

## How the sessions share one checkout

Every worker edits the same folder, on the one branch the definition names. There is no
separate copy per session. That is what lets a worker see the others' finished work as soon as
it is committed, and it is why the fleet's rules are strict about the tree:

- A worker changes only the files its task owns. The T1 worker may edit `install.md` and
  nothing the T2 worker owns.
- A worker never runs `git add`, `git commit`, `git push`, `git stash` or `git reset`, and never
  adds a branch or a worktree. It commits with `fleet_commit` (or `mike fleet commit -m` in its
  pane).
- To see what other sessions changed, a worker asks mike (`what_changed`) instead of reading
  `git status`, which would show everyone's unfinished edits mixed together.

`fleet_commit` takes the files the task owns, builds the commit in a private index so nothing
else staged in the folder gets in, runs the repository's pre-commit hook, and marks the commit
with the session that made it (a `Fleet-Seat` line in the message). It then pushes, merging
first if the remote has moved on. When mike has no credentials for the remote, the worker
pushes from its own session.

mike watches the folder once a second and records every file that changes and which session
changed it, with a note when a session wrote outside its own task's files. If no task owns a
change after 10 minutes, mike proposes a new task for it.
When an uncommitted edit breaks the tree for everyone, mike works out which session's edit did
it and tells that session. If it is still broken 10 minutes later, the reference session is told
as well.

## Checks run by mike, not by the workers

The definition names the project's check (`check.command`, for example `npm run typecheck`).
mike runs it on every new commit, in a clean copy of that commit kept under
`.claude/worktrees/` in your repository. A worker's unfinished edits are never part of the
result, and two checks can run at the same time. When a check fails, mike reads the file
names in its output and tells the sessions that own those files.

Workers do not run the full check themselves. They ask mike for a result (`fleet_check`), which
reuses a result already recorded for the same commit or joins a run that is already going. A
worker that runs the check's own command anyway is reminded once to ask mike.

The gate command runs the same way on every pushed commit, in its own clean copy. Its failures
go to the sessions that own the files named in the output, which tells mike whom to contact,
not who caused the failure.

A project can declare its slow commands and what they may run alongside in
`.claude/fleet/operations.json`, so that, for example, two builds that share a cache never run
at once. A project that declares nothing gets no such limits.

## How a task is accepted

A worker finishes a piece of its task with a commit and then a report (`fleet_report`). The
report names the commit, the outcomes from the definition it serves, the command the worker
ran to show the change working with its exit code, the tests it changed, and anything it could
not establish. mike refuses a report with any of those missing.

mike then checks the report in two steps:

1. If the task says what proves it is finished (`done_when` in the definition: commands with the
   exit code they should give, files that must have changed, a test that must have been added,
   a command and the output it should print), mike runs those at the reported commit. When one
   fails, the worker is told which, and no model reads the report.
2. A judge, a single model call, reads the commit's diff, the report, the task's brief and the
   outcomes it serves. It does not see the worker's own summary. It answers pass or fail, and a
   fail names the one thing the worker must do next. When one fact is missing, the judge may
   first ask a checker, another model call, one narrow question about one file or output.

The worker is mailed the verdict. A pass on every outcome a task serves makes the task
accepted, and tasks that were waiting for it start. A fail goes back to the same worker, which
fixes that one thing before anything else and reports again. On a later report the judge asks
whether that one thing is now done and whether anything that passed before has broken, so a
worker is not handed a new demand every round.

In the example, T1's worker commits `install.md` and reports that it served "Homebrew first on
the install page". The judge reads the diff, finds the Homebrew section above the others, and
passes it. T3's worker starts on its next round.

The judge, the checker and the problem-solver below use the model and key you set for
steerers on the settings page (see [Steering](../steering.md) and [Settings](../settings.md)),
and they send the diff and the report to that model's provider (see [Privacy](../privacy.md)).
Without a model and key, reports wait unjudged, each worker is told so once, and they are judged
as soon as the key is set.

## Questions while it runs

A worker that needs something asks rather than guessing: another session, the session that owns
a file, or the reference session. A question from a blocked worker that is still open after 5
minutes goes to the reference session. A worker that needs you writes a note marked for you,
and the status screen shows it under NEEDS YOU; it never stops on a question in its own pane,
because the fleet would wait behind it. Run `mike fleet focus <fleet>` to bring that session to
the front and answer there.

A worker can also add a task to a running fleet (`fleet_propose`), when every file it names
exists or is marked new and no open task owns any of them, or move part of its own task into a
new one (`fleet_split`). mike starts a worker for the new task on its next round.

## When something goes wrong

mike watches every session in the fleet and does what you would otherwise do by hand:

| What happens | What mike does |
|---|---|
| A worker sends nothing within 90 seconds of being opened | Closes it and opens it again, up to twice; then shows it under NEEDS YOU |
| A worker's process has ended | Records it closed and opens a new worker for the task |
| A worker has unread mail and has been silent for 5 minutes | Wakes it over Remote Control; if it is still silent 2 minutes later, replaces it |
| A check runs past its time limit | Stops it and queues it again once; a second time, it counts as a failure for the task |
| The reference session passes 600,000 tokens of context | Opens a new reference session to take over |
| A session will not close when the fleet stops | Tries three times, then records it closed and shows it under NEEDS YOU |

Problems that need a decision rather than a restart are given to a problem-solver, a model call
that reads what happened and picks one action. Examples are a check failing in files no task
owns, a worker disputing a fail, an outcome failing three times, every session sitting idle, or
the first run of the gate command failing. The action can be a message to the workers, a change
to the definition, a new task, a narrower rerun of a check, or a question to you. A problem gets
at most three decisions, and the third goes to you whatever it says. A run hands at most 20
problems to the problem-solver.

## How it ends

When every task is accepted, mike stops the fleet: it closes each worker's pane, records the
run, closes the status pane and the workspace it opened, and marks the fleet `done`. A fleet
whose sessions have all exited on their own ends the same way. `mike fleet stop` ends it early.

The record of the run holds four numbers, which `mike fleet record <fleet>` prints: the time to
done, the minutes lost to faults (relaunches, reports waiting for a verdict, checks past their
limit), the tokens spent per accepted task, and how often you had to step in. `--against` with
another fleet's slug compares two runs.

After the close, the retrospective session reads the run and writes its findings as notes. Turn
it off with `retrospective.enabled: false` in the definition.

A worker that learns something the next run should know from the start, such as a build that
cannot run twice at once, can write it as a note marked to carry over. mike offers it to you as
a lesson (see [Memory](../memory.md#keep-a-correction-as-a-lesson)). Once you approve it, it is
added to the reading of that kind of work's template in later fleets, so every worker copied
from it starts with it.
