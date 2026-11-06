---
title: Modality bench
status: active
created: 2026-11-06
updated: 2026-11-06
summary: One ask, five ways of running Claude Code on it (a single session, a session with subagents, an agent team, a workflow, a mike fleet), each in a sealed cell so no arm can learn from another, graded against hidden answers and measured from mike's capture.
---

# Modality bench

The question: given the same coding ask, how do the five ways of running Claude Code
compare on **whether the work gets done**, **how long it takes** and **what it costs**?

| Arm | What runs | How the work is split |
|---|---|---|
| `single` | one `claude -p` session | the session does everything itself; `Agent`, `TeamCreate` and `Workflow` are denied |
| `subagents` | one `claude -p` session | the session delegates with the `Agent` tool; `TeamCreate` and `Workflow` are denied |
| `team` | one `claude -p` lead, in-process teammates | the lead forms an agent team (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`); `Workflow` is denied |
| `workflow` | one `claude -p` session | the session writes a Workflow script that fans out subagents; `TeamCreate` is denied |
| `fleet` | mike's fleet: one seat per task | the definition is `scripts/fleet-refs/node.json`, started through the inspect API as `scripts/fleet-ref run` does |

Until now these have been compared indirectly, from runs on different asks at different
times. This bench puts the same ask through every arm under one protocol. The harness is
`scripts/modality-bench`; this page is the protocol it implements and the reasons.

## The ask

Every arm gets the five tasks of the smoke reference fleet
(`crates/mike/tests/fixtures/fleet-ref-node/`, defined in `scripts/fleet-refs/node.json`):
a small TypeScript package that needs `slugify`, `clamp`, `titleSlug` (after `slugify`),
`roundTo` and `formatMoney`, each with a test. The fixture already carries three seeded
traps, which is what makes it a test of judgement and not only of typing: T3 depends on T1,
T4's brief leaves out the rounding rule that `docs/rounding.md` states, and T5's brief names
a file that does not exist.

The ask is **rendered from the definition**, never written by hand, so the five arms read
the same words: the goal, each task's name, brief, files and the task it waits for, the
project check, and the two rules (nothing outside a task's files changes; nobody runs
`npm install`). The fleet arm gets the definition itself, one brief per seat, because that
is what a fleet is; the four `claude -p` arms get the rendered text as one prompt. The only
text that differs between arms is one **modality line**, fixed in the plan and printed in
the report:

| Arm | Modality line |
|---|---|
| `single` | Do all of this yourself in this session: do not start subagents, teams or workflows. |
| `subagents` | Use the Agent tool to delegate the tasks to subagents, independent ones at the same time. |
| `team` | Create an agent team and give each task to a teammate; the teammate for T3 waits for T1. |
| `workflow` | Use a workflow to orchestrate these tasks with subagents. |
| `fleet` | none: the definition's tasks and briefs are the split |

Every arm is told the same finish line: commit to `main` and push to `origin`. Grading reads
what reached the origin.

The fleet definition gives the split (five tasks, files and `after`) to the fleet for free,
and the rendered ask gives the same split to the other arms. That is deliberate: the bench
compares how the modalities **execute** a known decomposition, not how well a model invents
one. A bench of decomposition would hand every arm only the goal; it would be a different
bench with a different ask.

## The cells

A **cell** is one arm run once. A plan holds `reps` cells per arm, in a seeded random order,
and the harness runs them **one at a time**: an arm that shared the processor with another
arm's build would be measured against the other's load. Each cell is a directory the harness
makes for it and never reuses:

```
<plan dir>/cells/<arm>-<rep>/
  work/          the working copy: fresh copy of the fixture, node_modules installed
  origin.git     its own bare origin; grading reads main here
  answers/       the known answers, a separate repository the arm is never pointed at
  claude-home/   CLAUDE_CONFIG_DIR for the claude -p arms (empty before the run)
  ask.md         the exact prompt the arm was given
  cell.json      what was launched, when, exit, stop cause
  stdout.json    claude's --output-format json result, or the fleet's final status
```

`scripts/fleet-ref prepare node --dir <cell>` makes `work`, `origin.git` and `answers`, so
the fleet arm and the other four start from byte-identical trees. The plan directory must
not be under `/tmp` or a `scratchpad` folder: mike classifies a session at such a path as
`synthetic` (`dossier/build/session_class.rs`), and the bench wants its cells classified
like real work. The default is `~/.local/state/modality-bench/<stamp>/`.

## What is sealed, and how

The point of the cells is that **no arm can learn from another, and no run can learn from an
earlier one**. Each row names a way that could happen and what closes it.

| Channel | How one arm could reach another | Closed by |
|---|---|---|
| The repository | an arm reads another's files, commits or branch | a fresh copy and a fresh bare origin per cell; cells are never reused |
| The answers | an arm finds the reference solution | the answers are a branch of a separate repository that is not a remote of the working copy and not under it (`fleet-ref prepare`); graders read them, arms never see the path |
| Claude Code's home | user `settings.json`, hooks, MCP servers, `CLAUDE.md`, the auto-memory for the project path, transcripts of an earlier cell | `CLAUDE_CONFIG_DIR=<cell>/claude-home` for the four `claude -p` arms, holding only the operator's sign-in (`.credentials.json` copied, `oauthAccount` carried into a fresh `.claude.json`, both 0600) and, for the team arm, `teammateMode: in-process` in that `.claude.json`; `--setting-sources project` so only the fixture's own `.claude/` is read; `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`; `--strict-mcp-config` with no MCP config so the arm has no MCP tools |
| Transcripts and resume | a cell resumes or forks an earlier conversation | a plan-minted `--session-id` per cell; the home directory is per cell, so there is nothing to resume |
| mike's memory | lessons, recall, steerer corrections, directives, project knowledge from an earlier cell | all are scoped by `project_path`, which is the cell's `work` directory, so a fresh path has none; the fixture carries no `.mike/knowledge.json`; the harness also refuses to run while `reminders.inject_lessons`, `recall_injector`, `agent_lessons`, `workspace_briefing`, `self_identity_reminder`, `steerer` or `turn_guard` is on, because a bench of Claude Code's modalities is not a bench of mike's steering |
| mike's rewrites | a filter rule, weasel intercept, proxy tools or compression changes what the model sees | the harness refuses while any filter rule exists or `weasel_intercept`, `proxy_tools`, `repeated_notices` or a compression toggle with a forward share above zero is on; everything else in `/api/settings` is hashed into the plan and the run refuses when the hash changes between cells |
| The model | model, effort, client version | pinned in the plan; passed as `--model` and `--effort` to every `claude -p` and written into the fleet definition's `workers.ts.model`; `CLAUDE_CODE_SUBAGENT_MODEL` is set to the same model so subagents and teammates do not quietly run a cheaper one; `claude --version` is recorded and a cell refuses to start when it differs |
| The host | an arm runs while another arm, a deploy or a fleet is using the machine | one cell at a time under a lock file; the harness refuses to start a cell while `GET /api/fleets` lists a running fleet or `.deploy/deploy.lock` exists; `nproc`, free memory and the load average are written to `cell.json` at start and end |
| Order | the arm that always runs first pays the cold start | the cell order is a seeded shuffle across arms and reps; the seed is in the plan |
| The clock | a slow arm runs until it succeeds | one wall-clock cap per cell (`--cap`, default 2400 s, the fleet reference's); a cell past it is stopped and graded on what landed, stop cause `timeout` |

Two channels stay open, and the report says so every time:

- **The prompt cache.** Every arm shares Anthropic's cache for Claude Code's system prompt and
  tool list with whatever ran before it. Nothing on the client closes this. The shuffle
  spreads it evenly over arms; the report shows cache reads and cache writes as separate
  columns and never folds them into one "tokens" number without saying how.
- **The fleet's user scope.** mike launches seats with the operator's `~/.claude/` (its MCP
  server, the `fleet-worker` skill and the fleet agents live there, and `XDG_CONFIG_HOME`
  is moved but `CLAUDE_CONFIG_DIR` is not). The fleet arm therefore sees the operator's user
  settings and user `CLAUDE.md`, and the other arms see neither. The harness hashes
  `~/.claude/settings.json` and `~/.claude/CLAUDE.md` into the plan so the asymmetry is at
  least the same across reps, and names it in the report.

## Modality fidelity

An arm told to use teammates that quietly does the work itself is not a measurement of
teams. After each cell the harness reads mike's capture for the cell's `project_path` and
checks that the modality actually fired; a cell that did not is reported with
`fidelity: false` and kept out of the medians.

| Arm | Evidence in capture |
|---|---|
| `single` | one session; every request on the primary lane (`turn_origin` `primary`/`primary_first`, no `x-claude-code-agent-id`); no `Agent`, `TeamCreate` or `Workflow` tool use |
| `subagents` | requests with `turn_origin = subagent` and `Agent` tool uses; none `workflow_subagent` |
| `team` | more than one session at the project path, or `<teammate-message` wrappers in the lead's requests, and a `TeamCreate` use |
| `workflow` | a `Workflow` tool use and requests with `turn_origin = workflow_subagent` |
| `fleet` | `launch.launches` rows for the run id and `session_class = fleet_seat` for the seats |

The tool denials (`--disallowed-tools`) make the wrong modality impossible; the fidelity
check makes a missing one visible.

## Grading

The grader reads **only the tree that reached `origin.git`'s `main`**, checked out into a
scratch directory with the cell's `node_modules` linked in, and never the model's words.
Per task it asks three things, in order; the first that fails is the verdict:

1. **Hidden tests pass.** The known answer's test file (`answers/<task>/test/…`) is copied
   over the arm's test of the same name and run with `node --test` against the arm's
   `src/`. This is what catches T4's half-away-from-zero rule and T5's wrong file.
2. **The arm's own check passes.** `scripts/check` in the checked-out tree: the type check
   and every test the arm wrote.
3. **Scope held.** Every path changed since the seed commit is one some task owns (`files`
   or `new_files`); an arm that commits all five tasks in one commit is judged on the union,
   since nothing ties a path to a task there. Two traps are checked by name: `src/format/`
   does not exist (T5), and `src/title.ts` imports from `./slug.ts` (T3).

The tree-level type check runs once; a failure fails every landed task, because the fleet's
own check would. A task is `accepted` when all three hold, else `failed: <which>`. A cell's
score is tasks accepted out of five; `landed` counts tasks whose files reached the origin at
all, and `in_tree` grades the working copy the same way (its uncommitted work committed in a
copy first) for an arm that did the work and did not push, reported separately and never
added to the score.

## What is measured

Everything comes from mike's capture, joined on the cell's `project_path`, the same way
`scripts/compressor-task-trials` and `.claude/fleet/outcome-checks.sh` read it:

| Measure | From |
|---|---|
| `accepted`, `landed`, `in_tree` | the grader |
| `wall_seconds` | the harness clock, launch to exit or cap |
| `sessions`, `requests`, `tool_calls` | `sessions.sessions`, `requests.requests`, `tools.uses` at the project path |
| `fresh`, `cache_write`, `cache_read`, `output` | `responses.responses`, summed by lane (`primary`, `subagent`, `workflow_subagent`, sidecars) |
| `weighted` | the same weights as the compressor trials: fresh 1.0, cache write 1.25, cache read 0.1, output 5.0; a relative cost, not dollars |
| `fidelity` | the table above |
| `stop` | `done`, `timeout`, `error`, `budget` |
| fleet only | the four numbers of the run record: time to done, fault minutes, tokens per accepted task, manual interventions (`mike fleet record`) |

Fleet leaf calls (the judge, the checker, the problem solver) go to mike's own provider,
not through the proxy, so they are not in the fleet cell's token columns. The report marks
the fleet row `leaves: unmeasured` unless `launch.run_records` carries them.

`collect` writes `results.json` and `report.md`: one row per cell, then per arm the median
and range of each measure over the cells with `fidelity: true`, and a line for every
channel that stayed open. With `reps` under three it prints the rows and no medians.

## Running it

```sh
scripts/modality-bench plan --dir ~/.local/state/modality-bench/first \
    --model sonnet --effort medium --reps 3 [--arms single,subagents,team,workflow,fleet] [--seed N]
scripts/modality-bench run --dir ~/.local/state/modality-bench/first      # resumes; one cell at a time
scripts/modality-bench grade --dir … [--cell team-2]                      # re-grade without re-running
scripts/modality-bench collect --dir …                                    # results.json and report.md
scripts/modality-bench dry-run --dir …                                    # every cell with a scripted agent, no model
```

`plan` freezes the ask, the modality lines, the cell order, the session ids, the model
pins, the settings hash and the client version, and refuses to overwrite a plan. `run`
skips cells that have a `cell.json` with an exit, so a stopped run continues where it was.
`dry-run` applies the known answers with a scripted agent and proves the plumbing and the
grader without a model call; a dry run must grade every cell 5 of 5 before a real run
means anything.

Needs: mike running with its database, the proxy at `ANTHROPIC_BASE_URL`
(`MIKE_PROXY_URL`, default `http://127.0.0.1:9988`), the inspect API (`MIKE_INSPECT_URL`),
`psql` and `MIKE_DATABASE_URL` or the `.env` beside the script, `claude`, `node` and `npm`,
and for the fleet arm a terminal backend `mike spawn doctor` passes.

## What this bench does not say

- It is five small TypeScript helpers with a given split. A result here is about
  coordination overhead and fidelity on a task that fits one context window; it says
  nothing about a change that does not.
- `accepted` is binary per task against hidden tests. It does not grade code quality, the
  tests the arm wrote beyond "they pass", or the commit messages.
- Weighted tokens are a relative cost under one weighting, not a bill, and the prompt cache
  is shared across arms.
- A fleet's seats run with the operator's user-scope Claude Code configuration; the other
  arms run with none. The report names this on every run.
- Three reps give a median and a range, not a confidence interval. Treat a difference
  smaller than the range as no difference.

## See also

- [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md#reference-fleets): the reference fleet the fixture
  and the fleet arm come from, and the run record the fleet row reports.
- [`FLEET_ISOLATED_CHECKS.md`](FLEET_ISOLATED_CHECKS.md): how a fleet's own checks are
  isolated; the bench grades outside the fleet, in its own scratch checkout.
- `scripts/compressor_eval/README.md` and `scripts/compressor-task-trials`: the paired-trial
  design (frozen plan, scripted dry run, capture read by session) this harness follows.
