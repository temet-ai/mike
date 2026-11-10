# Write a fleet definition

A fleet's definition is everything mike needs to run it: the goal, the outcomes that prove it
is met, the tasks and the files each one owns, and the commands that say whether the project is
sound. The `/fleet` command writes it for you (see [Run your first fleet](first-fleet.md)), and
this page explains each part so you can read what it wrote, correct it, or write one yourself.
The definition lives in mike's database, and Claude reads and changes it with the
`define_fleet` and `get_fleet` tools (see [MCP](../mcp.md)).

## The parts

| Part | What it is |
|---|---|
| Goal and ask | One sentence for what is true when the fleet is done, and your own words, verbatim |
| Repository and branch | The checkout and the single branch every session commits to |
| Criteria | Three to seven outcomes, each with the evidence that settles it and the files involved |
| Workers | A model, and optionally a specialist agent, for each kind of work (for example `backend`, `web`, `docs`) |
| Tasks | For each: a name, its kind of work, the criteria it serves, the files it may touch, up to three tasks it must wait for, and a brief |
| Check | The command that says whether the tree is sound, and the paths its answer depends on |
| Gate | A quick command mike runs on every pushed commit, and once on a clean copy before any session opens |
| Rules | Anything no session may do in this run in particular |

## Outcomes the judge can check

Each criterion has the text you asked for and a check: what someone would look at to know it is
true. The judge decides from the commit's diff, the command the worker ran and this check, so
a check that names something you can observe gets a fair verdict, and a vague one does not.

| Weak | Better |
|---|---|
| "The install page is good" | "The Homebrew section is the first install method on the built `/install` page" |
| "Links work" | "`scripts/check-links` exits 0 on the built site and 1 on a page with a broken link" |
| "Tests pass" | "The test for an empty search returns no rows, and it fails on the commit before" |

A criterion also lists the files involved, traced from where the behaviour starts to where it is
done. Every one of those files must belong to some task, and mike refuses a definition where one
does not.

## Tasks and the files they own

A task is one piece of work for one session. Give each file to one task: two sessions editing
the same file in one folder overwrite each other. For the example:

| Task | Criteria | Files | After |
|---|---|---|---|
| T1 Install page | C1 | `install.md` | |
| T2 Quickstart | C2 | `quickstart.md` | |
| T3 Link check | C3 | `scripts/check-links`, `tests/links/` | T1, T2 |

`after` names up to three tasks this one cannot start without. A task waits for those tasks to
pass, not for the whole fleet to finish. Start as much as possible at once and keep `after` for
real dependencies.

When two tasks that are not done have files in the same compile unit (one Cargo package, or one
`package.json` workspace member), mike warns before the start, because one broken edit stops
the build for both. Give that unit to one task, or order the tasks with `after`.

## Briefs

A task's brief is all its session knows about the work beyond the fleet's rules and the
definition. Write it so it stands on its own: what to change, which criteria it serves, what to
read first, and how to show the change working.

```
C1: rewrite install.md so Homebrew is the first install method, then mise, then the archive.
Read the current install.md and README.md first; keep the commands exactly as they are.
Build the site with `npm run build` and open /install to show the order.
```

## Saying when a task is finished

A task may say, in `done_when`, what proves it is finished. mike runs these at the commit the
worker reports, before any model reads the report:

| Field | What mike checks |
|---|---|
| `checks` | Each named operation (declared in `operations.json`) exits with the expected code |
| `files_changed` | These files changed in the commit |
| `tests_added_under` | A test was added under this path |
| `exercise` | A command, and the output it must print |

For T3 in the example:

```json
"done_when": {
  "files_changed": ["scripts/check-links"],
  "tests_added_under": ["tests/links/"],
  "exercise": { "command": "scripts/check-links site/dist", "expect_output": "0 broken links" }
}
```

A task without `done_when` goes straight to the judge. See [How work is accepted](acceptance.md).

## The check, the commit check and the gate

- `check.command` is the one command that says whether the tree is sound, such as
  `npm run typecheck` or `cargo check --workspace`. `check.paths` lists what its answer depends
  on. mike runs it on every new commit in a clean copy (see
  [Share one checkout](shared-checkout.md#checks-run-by-mike)).
- `commit_check` is an optional formatter in check-only mode, run on the files of each commit.
- `gate.command` is the quick form of the project's full check. mike runs it on every pushed
  commit, and once on a clean copy of the branch before any session opens. It must not change
  files. When that first run fails, only a repair task named in `baseline.repair_task` may
  start, until the gate passes.

A project can keep these in `.claude/fleet/project.md`, a short file saying how it is built and
checked, which `/fleet` reads, and declare its slow commands in `.claude/fleet/operations.json`
(see [Share one checkout](shared-checkout.md#declared-operations)).

## Workers, models and goals

`workers` names each kind of work, the model its sessions use, and optionally the project's own
specialist agent for that area (for example a `frontend-developer` agent in `.claude/agents/`).
mike adds its `fleet-worker` skill to every worker. Use a small model for simple tasks and a
larger one for hard tasks; a task that fails on the small model can be run again on a larger
one.

`goals.workers`, or `goal` on one task, is a sentence saying when a worker's task is done, with
`{task}`, `{name}` and `{criteria}` filled in for each worker. mike hands it to Claude Code's
`/goal`, which keeps the session working until it holds; see
[How a fleet runs](how-a-fleet-runs.md#goals).

## A whole definition

The example, as `/fleet` would write it:

```json
{
  "goal": "The install page puts Homebrew first, the quickstart matches it, and a link check guards the docs.",
  "ask": "Rewrite the install page and the quickstart, Homebrew first, and add a link check",
  "repo": "/home/me/projects/site",
  "branch": "main",
  "check": { "command": "npm run build", "paths": ["install.md", "quickstart.md", "src/", "package.json"] },
  "criteria": {
    "C1": { "text": "Homebrew first on the install page", "check": "The Homebrew section is the first install method on the built /install page.", "paths": ["install.md"] },
    "C2": { "text": "the quickstart matches", "check": "Every install command in quickstart.md appears on the install page.", "paths": ["quickstart.md"] },
    "C3": { "text": "add a link check", "check": "scripts/check-links exits 0 on the built site and 1 on a page with a broken link.", "paths": ["scripts/check-links", "tests/links/"] }
  },
  "workers": { "docs": { "model": "sonnet" } },
  "goals": { "workers": "task {task} ({name}) has a commit and a fleet_report for {criteria}; or stop after 30 turns" },
  "rules": ["No session edits the site's theme or navigation."],
  "tasks": {
    "T1": { "name": "Install page", "discipline": "docs", "criteria": ["C1"], "files": ["install.md"], "after": [],
            "brief": "C1: rewrite install.md so Homebrew is the first install method, then mise, then the archive. Keep the commands as they are. Build with npm run build and open /install to show the order." },
    "T2": { "name": "Quickstart", "discipline": "docs", "criteria": ["C2"], "files": ["quickstart.md"], "after": [],
            "brief": "C2: make every install command in quickstart.md one that install.md shows, in the same order. Read install.md on the branch first." },
    "T3": { "name": "Link check", "discipline": "docs", "criteria": ["C3"], "files": ["scripts/check-links", "tests/links/"], "after": ["T1", "T2"],
            "brief": "C3: write scripts/check-links, which reads the built site and exits 1 on any broken link. Add a test in tests/links/ with one good and one broken page.",
            "done_when": { "files_changed": ["scripts/check-links"], "tests_added_under": ["tests/links/"],
                           "exercise": { "command": "scripts/check-links site/dist", "expect_output": "0 broken links" } } }
  }
}
```

## Before it can start

The definition cannot start while anything is missing. Ask Claude for the fleet's state
(`get_fleet`): it lists what is missing and, before the start, what this machine would refuse,
such as more sessions than the terminal can open, or a branch mike cannot push to. Fix those,
then mark the fleet ready. If **Start fleets on their own** (`fleet_auto_start`, on by default)
is off in [Settings](../settings.md), the fleet then waits for `mike fleet start <fleet>`.
