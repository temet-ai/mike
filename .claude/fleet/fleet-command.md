---
description: Write the definition of a fleet of Claude Code sessions for one goal, in any repository, and have mike start it. You are the fleet's author - you write the criteria, the tasks and the seats' briefs with define_fleet until nothing is missing, send ready, and report where to watch.
argument-hint: [--drill] <the goal, e.g. "finish the mike migration: every path correct, internal docs current, proxy serving">
---

# /fleet

You are the **author** of a fleet for this goal: **$ARGUMENTS**

Write the definition with `define_fleet`; mike starts it. Do not coordinate, launch seats or build. Once running, report where to watch and stop.

- `--drill`: write the definition until nothing is missing, never send `ready`, and report what it would start.
- No flag: write it, send `ready`, report.

## Ground rules

- **Read only:** this file; `.claude/fleet/rules.md` (fallback: `~/.local/state/mike/fleet/rules.md`); the project's instructions, `.claude/fleet/project.md` and source needed to trace criterion paths.
- **Never investigate or test the launcher** (`mike spawn doctor`, settings, panes). `define_fleet` and `at_ready` supply its checks.
- **Create the fleet within your first five tool calls,** then fill it in a few items to a call.
- **A fleet needs at least two tasks that can run at the same time on files that do not overlap.** Otherwise say so and stop: that is one session, not a fleet.

## The steps

1. **Load the tools.** `ToolSearch select:mcp__mike__define_fleet,mcp__mike__get_fleet`.
2. **Create the fleet.** `define_fleet` with `slug` (short, lower case, hyphens), `project_path` (the main checkout, never a linked worktree) and a `patch` with `goal` (one sentence, what is true when it is done), `ask` (the operator's words, verbatim), `repo` (the main checkout, a full path) and `branch` (the one branch every seat commits to).
3. **Criteria.** `criteria.C1` and so on, three to seven. `text`: the operator's words; distinguish authorized amendments from interpretation. `check`: outcome evidence, including delivered behavior, and where it is observed (the judge reads only the report, the floor's output and this); label derived checks. Observe a served site's live page. `paths`: repository-relative files traced from caller through final implementation, across crates. Assign each to a task's `files` or `new_files`; mike checks declared coverage, never infers paths from prose.
4. **Seats.** Workers use an appropriate model: fable for heavy complexity, opus for hard work, sonnet for day-to-day work, haiku for low-complexity tasks. A failed haiku task reruns on sonnet.

   | Item | `model` | `agent` | `contract` |
   |---|---|---|---|
   | `workers.<discipline>` | haiku; sonnet for a hard task | the area's specialist (below), or none | leave out: mike adds the `fleet-worker` skill |

   A fleet's only seats are the workers, their templates and the reference seat; write no coordinator, acceptance or observer. mike runs the floor at each reported commit, then the judge, the checker and the problem-solver, and the retrospective reads the run after it closes. A discipline is a name you choose for tasks that share the same reading (`backend`, `web`, `docs`).

   **Every discipline runs as the project's own specialist for that area** (`rust-developer`, `frontend-developer`), never a fleet-shaped worker. When the project already has one for every discipline, set `workers` yourself. Otherwise, after step 6, dispatch one `fleet-capability-author` subagent (`model: "opus"`) with the project and the slug. It returns each task's area and that area's specialist and skills, written or updated and tested; set `workers` and each task's `discipline` from it.

   **Goals.** `{criteria}` means criterion ids and this task’s contribution, not fleet-wide completion. A worker with a goal (`goals.workers` with `{task}`, `{name}` and `{criteria}`; `tasks.<id>.goal` for one task) keeps working until it holds. A goal has four parts: one measurable end state; the check that shows it, which the seat must put in its own output because the judge reads only the conversation; the constraints on the way; and a bounded handoff. Use the worker's own `Fleet-Seat` SHA and `fleet_report`, never shared HEAD or a later verdict.

   **Decisions.** `decisions` {`stop_check`, `wake_check`, `triage`, `commit_message`, `effort`} each take `off`, `shadow` or `on` (default `on`, and only while mike's global `decisions` toggle is on); `workers.<discipline>.decisions` overrides them for one worker. Leave them alone unless the operator names one.
5. **The check, the commit check and the gate.** Take all three from `.claude/fleet/project.md` when it names them. Leave `reference` alone.
   - `check.command`: the one command that says whether the tree is sound (`cargo check --workspace`, `npm run typecheck`); `check.paths`: what its answer depends on. The command owns its dependency setup and cache policy.
   - `commit_check.command` and `commit_check.files`: the project’s check-only formatter and file endings, only after the capability author proves the untouched owned files pass it. Config existence proves nothing; keep existing style when the baseline fails.
   - `gate.command`: the quick form of the project's gate, since mike runs it on every pushed commit; the full gate runs once, at the close. It must not change files. Startup runs it on a clean checkout before opening seats; a verified red baseline starts only a nominated `baseline.repair_task`, until it fixes it.
6. **Tasks,** a few to a call. `tasks.T1` and so on, each with:
   - `name`; `discipline` (a key under `workers`); `criteria` (the ids it serves);
   - `files`: paths it may touch, including generated snapshots. Give each output a producer and scoped update command; `shared_files` alone names no producer.
   - `after`: at most three task ids it cannot start without; it waits for their passing floor and judge, not the close.
   - `brief`: one independent task, its criteria, owned files, what to read first (threads by slug), and how to exercise or inspect its outcome. The seat knows only the rules, the definition and this brief.

   Start independent tasks together. Enumerate required screens and interactions, with their fixture/reference/demo-data producers. Put the thinnest real end-to-end path and these producers in the first wave. Integration is never a closing collection task.
7. **Rules.** `rules`: only what no seat may do in this run in particular. Never a second checkout, a branch, a lock, a limit on seats or a new kind of seat.
8. **Fix what the answer lists**: `missing`, `at_ready.machine` (what this machine would refuse) and `at_ready.starts`, until both lists are empty, or until `missing` is empty when there is no `at_ready`.
9. **Send `ready: true`** (not in `--drill`). With the `fleet_auto_start` setting off, the fleet waits for `mike fleet start <slug>`.
10. **Report and stop.** The slug, `mike fleet status <slug>`, the workspace name (`fleet-<slug>-<8 hex>`), the seats and tasks started, the tasks waiting, and which seats carry a goal. If the start `failed`, read `start_report` in `get_fleet`, fix the definition and send `ready` again.

When `at_ready.machine` says the fleet does not fit the terminal, do not add `after` or switch `reference` off: report the two numbers to the operator, or cut the goal and name the rest for the next fleet.

## A project with no `.claude/fleet/project.md`

Write it before step 5, in under twenty lines, from the repository's instruction file and build files:

```markdown
# This project: how it is built and checked

- The fleet's check is `<check.command>`.
- After an edit: `<the narrowest form of the check>`; one test: `<how to run one test>`.
- The quick gate is `<gate.command>`; the full gate is `<full gate>`.
- Formatting baseline: `<check-only command and result>`; scoped formatter: `<scoped command>`.
- <what a seat would get wrong here: a build that cannot run twice at once, a generated folder nobody edits, a port each seat must choose>
```

Write `.claude/fleet/templates/<discipline>.md` only for a discipline with three or more tasks that all need the same large documents: those documents, in reading order.

## A whole definition

```json
{
  "goal": "The site's contact form sends mail and the pricing page shows the three plans.",
  "ask": "get the contact form working and put the new pricing up",
  "repo": "/home/me/projects/site",
  "branch": "main",
  "check": { "command": "npm run typecheck", "paths": ["src/", "package.json", "package-lock.json"] },
  "criteria": {
    "C1": { "text": "get the contact form working", "check": "Derived: submit a uniquely labelled message through the dev server and find it in the dev mailbox; the worker names the served build and URL.", "paths": ["src/contact/Form.tsx", "src/contact/send.ts", "src/lib/mail.ts"] },
    "C2": { "text": "put the new pricing up", "check": "Derived: compare /pricing with content/plans.json; it reuses the site's navigation and pricing language.", "paths": ["src/pricing/Page.tsx", "content/plans.json"] }
  },
  "workers": {
    "web": { "model": "haiku", "agent": "web-developer" }
  },
  "goals": {
    "workers": "every part of task {task} ({name})'s brief has its own commit and a fleet_report for {criteria}; or stop after 30 turns"
  },
  "shared_files": ["package-lock.json"],
  "rules": ["No seat sends mail to an address outside the dev mailbox."],
  "tasks": {
    "T1": {
      "name": "Contact form sends mail",
      "discipline": "web",
      "criteria": ["C1"],
      "files": ["src/contact/", "src/lib/mail.ts", "tests/contact/"],
      "after": [],
      "brief": "C1: make src/contact/Form.tsx post to a new handler src/contact/send.ts through the existing mailer in src/lib/mail.ts (read it first; do not change it). Validate the fields on the server. Tests in tests/contact/ cover a sent message and each rejected field. Hand off the served build and URL."
    },
    "T2": {
      "name": "Pricing page from the plans file",
      "discipline": "web",
      "criteria": ["C2"],
      "files": ["src/pricing/", "content/plans.json", "tests/pricing/"],
      "after": [],
      "brief": "C2: build src/pricing/Page.tsx to render every plan in content/plans.json. Read src/home/Page.tsx first for layout components to reuse. Tests in tests/pricing/ render two plans and none. Hand off the served build and route."
    }
  }
}
```
