# Run several sessions on one goal

A fleet splits one goal into tasks and gives each task to its own Claude Code session, all
working at the same time in one checkout. You state the goal and what would prove it is
met; mike opens the sessions, checks what they commit, and tells you when it is done or when
it needs you. A fleet leaves a single session alone: a session you start with
`mike spawn` is not part of a fleet.

Use a fleet when the goal splits into at least two tasks that touch different files and can
run side by side, such as a backend change, the page that uses it and the docs for both.
For one task a single session is faster; see [Sessions](sessions.md).

You define a fleet from inside Claude Code with `/fleet`, and watch it with
`mike fleet status` or in the terminal pane the fleet opens. The web UI's
[Fleet page](#see-a-projects-sessions-on-the-fleet-page) draws a project's sessions and
the messages between them; it does not show a fleet's tasks.

## Before you start

- A terminal that `mike spawn` can open panes in (see
  [Sessions](sessions.md#choose-a-terminal-first)). A fleet's sessions are ordinary Claude
  Code sessions opened the same way.
- mike running, with its database.
- A repository with a command that tells whether the tree is sound, such as a type-check or
  a build. The fleet runs it for every session.

The sessions' traffic goes to Anthropic like any other session's, and see
[Privacy](privacy.md). They commit and push to the one branch you name, in your checkout.
They do not create worktrees or branches.

## Run a fleet on the example site

The guide's fictional example continues here: the folder `~/projects/site`, where the
session `docs` is rewriting the install page. You now want the install page, the quickstart
and a link check done at once. The correction "Put Homebrew first on the install page." is
one of the things the fleet's install-page task must keep true.

1. In a Claude Code session in `~/projects/site`, run:

   ```
   /fleet Rewrite the install page and the quickstart, Homebrew first
   ```

   `mike serve` writes this command to `~/.claude/commands/fleet.md` when no file of that
   name exists; an edited or linked file is left alone. Add `--drill` before the goal to
   write the definition without starting it and see what would start.
2. That session is the fleet's author. It writes the definition and reports where to watch,
   then stops. It does not do the work.
3. Run `mike fleet list`. The new fleet is the top row, with its state and how many items
   its definition still lacks.
4. Run `mike fleet status <fleet>` (the slug from the list). When the fleet is ready it
   starts on its own, and the screen lists one task per page with the session working on
   it.
5. When the tasks are accepted the fleet reads `done`. Run `mike fleet record <fleet>` to
   see how long it took.

## What a definition holds

| Part | What it is |
|---|---|
| Goal and ask | One sentence for what is true when the fleet is done, and your own words, verbatim |
| Repository and branch | The checkout and the single branch every session commits to |
| Criteria | Three to seven outcomes, each with the evidence that settles it and the files involved |
| Workers | A model, and optionally a specialist agent, for each kind of work (for example `backend`, `web`, `docs`) |
| Tasks | For each: a name, its kind of work, the criteria it serves, the files it may touch, up to three tasks it must wait for, and a brief |
| Check | The command that says whether the tree is sound, and the paths its answer depends on |
| Rules | Anything no session may do in this run |

For the site, the install-page task owns `install.md`, the quickstart task owns
`quickstart.md`, and neither may touch the other's file. Write each task's brief so it
stands on its own: a session knows only the rules, the definition and its own brief. Give
each file to one task.

The definition cannot start while anything is missing. Ask Claude for the fleet's state
(`get_fleet`, see [MCP](mcp.md)): it lists what is missing and, before the start, what this
machine would refuse, for example too many sessions for the terminal. Fix those, then mark
the fleet ready.

## Start it

Marking the fleet ready starts it. If you turned off **Start fleets on their own**
(`fleet_auto_start`, on by default) in [Settings](settings.md), the fleet waits until you
run:

```sh
mike fleet start <fleet>
```

`<fleet>` is the fleet's slug or its id. On start, mike:

1. Runs the definition's gate command on a clean checkout, if the definition has one. A
   failing baseline opens no workers; only a repair task the definition names may run
   until the baseline passes.
2. Opens a status pane that redraws the fleet's state.
3. Opens one session per kind of work, loaded with the rules and the definition, and
   starts each worker from it so workers begin with that already read.
4. Opens a reference session, unless the definition turns it off, that answers the other
   sessions' questions about the system.
5. Starts a worker for every task whose waiting-for tasks are done, and checks again every
   20 seconds.

No more than 16 panes open by default (`launch_max_surfaces`).

## Watch it

| Command | Shows |
|---|---|
| `mike fleet list [--project <path>]` | Every fleet, newest first: slug, state, items missing, when it was created |
| `mike fleet status <fleet>` | One screen: what is missing, every task, every open session, the newest verdicts, what is blocked |
| `mike fleet status <fleet> --watch 5` | The same, redrawn every 5 seconds |
| `mike fleet summary` | One line for a status bar: the newest running fleet's progress |
| `mike fleet focus <fleet> [seat]` | Brings a session's pane to the front: the one you name, else the first blocked one, else the one with the newest failure. Add `--switch` to change workspace. |
| `mike fleet record <fleet>` | For a finished run: time to done, minutes lost to faults, tokens per accepted task. `--against <fleet>` compares two runs. |

Add `--json` to `list`, `status` and `record` for scripts. Every command takes
`--config <FILE>`. The full list of commands is in [Command line](command-line.md).

The status screen raises three flags:

- **NEEDS YOU** lists questions a session has asked you, with its name. Run
  `mike fleet focus <fleet>` to bring that session forward and answer there.
- **NOTHING IS MOVING** means every open session has been idle for a while.
- **PROBLEM** shows something mike is working through, and whether it was escalated to you.

A fleet is in one of these states: `draft`, `ready`, `starting`, `running`, `stopping`,
`failed` or `done`. Inside a fleet's own sessions the [fleet-comms mod](mods.md#fleet-comms)
shows the same state and the messages between sessions.

### See a project's sessions on the Fleet page

Open **Fleet** in the menu, or <http://localhost:9990/fleet>. It shows the sessions of one
project, chosen at the top, over the last 24 hours, in three views: **Relationships**
(a graph of sessions linked by the messages between them), **Board**, and **Notebook**.
It lists any captured sessions, fleet or not, and does not list a fleet's tasks or
verdicts; use `mike fleet status` for those. See [The web UI](web-ui.md) and
[Projects](projects.md).

If there is nothing to show, the page says so:

- **No projects captured yet**: run Claude Code through mike and send a prompt.
- **No sessions in this project's fleet**: no session activity or messages for that project
  in the window.
- **No active fleet activity in this window**: the Board has only old sessions with no
  messages. **See all sessions** opens [Sessions](reading-sessions.md).

## When it finishes

A session reports finished work with a commit. mike checks that commit with the project's
check, and a judge reads the session's report against the criteria it serves. A task is
done when every criterion it serves has a passing verdict and none is waiting on a new one.

The fleet is `done` when every task is accepted and the sessions have closed. mike then
closes the status pane and the workspace it opened.

## Stop it

- **You:** `mike fleet stop <fleet>` asks the fleet to shut down. It closes each open
  session's pane and prints, per session, whether the pane closed and whether its process
  has exited. The fleet reads `stopping` until they are gone, then `done`. A stopping fleet's
  definition cannot be edited.
- **A failed start:** the fleet goes to `failed`. Ask Claude for its state (`get_fleet`) and
  read the start report, fix the definition and mark it ready again.
- **A red baseline:** if the gate command fails before any session opens, only the named
  repair task runs.
- **The pane limit:** no new pane opens at the cap, and a task whose session could not start
  is shown as not started with the reason. Close a finished pane or raise
  `launch_max_surfaces`.

To end one session only, run `mike spawn kill --session <id>`.

## When there is no fleet

Every `mike fleet` command talks to the running mike. With mike stopped, `mike fleet list`
exits with `could not reach the inspect API`; start mike first (see [Setup](setup.md)).
`/fleet` writes no definition when your goal cannot be split into at least two tasks on
separate files; that is a job for one session.
