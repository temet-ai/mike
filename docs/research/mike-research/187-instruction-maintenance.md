---
title: "#187 instruction and memory maintenance: what is true now, options, recommendation, measure"
status: active
created: 2026-11-15
updated: 2026-11-15
summary: The always-injected layer for temet-proxy on omit is 38,280 bytes; 10 instruction/memory lines cite something that is gone; text-only rules are broken in 8-16% of turns without corrections clustering on them. Recommend a mechanical this-machine scheduled job beside learning_audit, with a model step only for the judgement checks.
session_id:
  - bb9196f7-d5f9-4985-a6cb-3fdbd1cef9e5
---

# #187: a maintenance pass over instruction files and memory

Fleet mike-research, seat design-T3. Measured on omit, 2026-11-15, read-only.

## What is true now

### Injected layer, temet-proxy on omit

Loaded into every session (bytes / lines):

| File | Bytes | Lines |
|---|---:|---:|
| `AGENTS.md` | 22,731 | 254 |
| `CLAUDE.md` | 0 | 0 |
| `.claude/rules/workflow-and-documentation.md` (no `paths:`) | 3,107 | 54 |
| `~/.claude/CLAUDE.md` | 593 | 6 |
| `~/.claude/rules/` chrome-profiles, file-changes, model-routing, worktrees | 3,545 | 66 |
| memory `MEMORY.md` (index, Claude Code loads ≤200 lines / 25 KB) | 8,304 | 74 |
| **Always on** | **38,280** (≈9.6k tokens) | 454 |

Added on the first matching file read (`paths:` frontmatter): `.rs` → agent-behavior-and-data 3,562 + rust-database-reliability 5,144 + visual-design-and-ui 3,177 = 11,883 more (50,163 total); frontend → frontend-development 3,210 + visual 3,177 + agent-behavior 3,562. The four retired stubs (`__retired__/**` paths, 1,078 B) never load. The memory folder holds 62 files, 100,640 B, read on demand; `MEMORY.md` has 61 pointers, none dangling, no file unindexed.

Largest single-area blocks in `AGENTS.md` (candidates for "belongs lower"): Repo tools table `AGENTS.md:221-245` 3,812 B; mbx build cache `AGENTS.md:123-155` 2,539 B; worktree sweep `AGENTS.md:156-169` 1,117 B; Database `AGENTS.md:208-220` 862 B.

### Stale citations

Scanned 105 files (AGENTS.md, project rules, project and user agents, `.claude/fleet/*.md`, user CLAUDE.md and rules, the memory folder) for backticked or linked paths. 117 candidates; 63 left after accepting any tracked file or folder whose path ends with the citation; hand-checked to **10 stale lines, 6 missing targets**. The other 53 were false positives (paths relative to the citing file's own folder, e.g. `~/.claude/agents/unit.md` → `~/.local/share/unit/facts.md`; example names like `foo.rs`; extensions; branch names; other repositories).

| Line | Missing target | Now |
|---|---|---|
| `.claude/fleet/mike-machinery.md:100` | `docs/FLEET_ISOLATED_CHECKS.md` | moved to `docs/fleets/isolated-checks.md` in 4b12d79c1 (2026-11-09) |
| `.claude/fleet/mike-machinery.md:15` | `config/app_state/accessors.rs` | `crates/mike-features/src/state/accessors.rs` |
| `.claude/fleet/mike-machinery.md:111` | `.claude/agents/mike-repair.md` | never in git |
| `.claude/fleet/mike-machinery.md:5` | `observer-contract.md` | not in `.claude/fleet/` |
| `~/.claude/agents/fleet-observer.md:10`, `:35` | `.claude/fleet/observer-contract.md` | missing: the observer role says its cycle "is in" this file |
| `.claude/agents/fleet-rule-auditor.md:15` and the `~/.claude` copy | `worker-contract.md`, `observer-contract.md` | missing |
| memory `subagent-briefing.md:11` | `.claude/rules/finishing-includes-landing.md` | missing |
| `AGENTS.md:190` | "`.claude/agents/` holds only the two fleet agents" | it holds 12 files (wrong claim, not a missing path) |

### Are lines acted on? (project 993478, 2026-11-01 to 11-15)

5,889 operator turns; 796 judged, 365 corrected (45.9%). Judged turns that ran Bash: 628, corrected 280 (**44.6% baseline**). A turn breaks a line when one of its Bash commands (`tools.uses`, joined on session and request range) matches the line's signature.

| Line | Turns breaking it | Share of 628 | Corrected among them | Operator prompts naming it |
|---|---:|---:|---:|---:|
| `~/.claude/rules/file-changes.md:8` Edit/Write, not `sed -i`/`perl -pi`/heredoc | 101 | 16.1% | 49 (48.5%) | 2 |
| `AGENTS.md:109` never poll (`sleep`, `tail -f`) | 53 | 8.4% | 18 (34.0%) | n/a (word "poll" too common) |
| `AGENTS.md:133` raw cargo refused by hook | 16 | 2.5% | 9 (56%) | n/a |
| memory never-kill-processes-by-name `:8` / `.claude/fleet/rules.md:44` | 7 | 1.1% | 3 | n/a |
| `.claude/rules/workflow-and-documentation.md:17` never `--no-verify` | 4 | 0.6% | 2 | n/a |

What this shows:
- A line backed by a hook is obeyed (raw cargo 2.5%). Lines that are only text are broken in 8-16% of turns.
- Corrections do not cluster on broken lines: 34-56% against a 44.6% baseline, on small counts. The polling line is broken most in turns that are corrected less. A judged correction belongs to a turn, not a line, so this check can say a line is ignored. It cannot yet say a line caused a correction.
- `file-changes.md:8` contradicts the harness: Claude Code's bypass-permissions system text tells the model that sed, heredocs and short scripts are fine. That is the likely reason for its 16%. A text pass can report this conflict but cannot fix it in the file.

### What mike runs today

- `crates/mike/src/scheduled_jobs/jobs.rs:231` lists 8 jobs, and none reads instruction files. `learning_audit` (`jobs.rs:52`) is weekly with scope `one_machine`. Its last run on 2026-11-13 took 15 s (12:33:32-12:33:47) and wrote 10 findings.
- `memory_review` (`jobs.rs:185`) is the scheduled `claude -p` Opus model. On 2026-11-15 it ran 6.5 min (05:15:12-05:21:44).
- `lesson_worker` nightly (`crates/mike-lessons/src/lesson_worker/nightly.rs:21`) works on DB projects (`discover_daily_projects`, `:170`) and does lesson promote, trim, relations and retire. It reads no files.
- `scripts/efficiency_report/prompts.py:28-44` measures bytes and duplicate paragraphs, but only for `CLAUDE.md`, `AGENTS.md`, the fleet-worker skill and `.claude/fleet/*.md`. It skips rules, user files and memory.
- `efficiency-auditor` has been dispatched 0 times through Agent/Task since 2026-10-01 (`tools.uses`). In practice, a check that needs a person to start it is not run.

## Options compared

| | (a) script job beside `learning_audit` | (b) extend `lesson_worker` nightly | (c) `efficiency-auditor` on a schedule |
|---|---|---|---|
| Form | Python under `scripts/`, a `Job` in `jobs.rs` | Rust step in `nightly.rs` | `claude -p` session like `scripts/memory-review` |
| Cost per run | seconds and no model call (`learning_audit` takes 15 s; the scan and SQL above took < 30 s) | seconds and no model call, inside the proxy | one Opus session, ~6-7 min (as `memory_review`) |
| Change cost | script plus test, no deploy | Rust under the 500/100-line limits (`nightly.rs` is 266 lines already), and each tweak needs a deploy | edit a brief in `/settings` |
| Machine scope | needs `this_machine`, like `learning_promote` (`~/.claude` differs on omit, the Mac and infra) | the worker is DB-wide and has to map projects to local paths | per machine |
| Catches: still true | yes, but raw precision is 10/63. It needs the folder-relative and other-repo resolution above | same | yes, with judgement |
| Catches: followed | yes, for lines with a command signature (5 tested) | same | can read transcripts, but costs tokens per line |
| Catches: belongs lower | size and section bytes only | same | yes |
| Catches: memory facts | dangling pointers and missing paths (0 and 1 today) | same, and records in consolidation decisions natively | dated faults and wrong facts |
| Catches: wrong claims (`AGENTS.md:190`) | no | no | yes |

## Recommendation

Take **(a)**: a `this_machine` scheduled job `instruction_audit` beside `learning_audit`. It runs the mechanical checks and the size measure, writes a per-project list with evidence (line, missing target, or turn ids), and feeds the weekly learning audit. Give the judgement checks to **(c)**, run only on that list: one bounded `claude -p` step after the job, which opens a PR for checked-in files, edits memory directly with a recorded decision (operator decision 2026-11-13), and proposes changes to `~/.claude` files. Leave out (b). It puts machine-local file work in a DB worker and makes every tweak a Rust deploy for no extra coverage.

Build the four checks in this order:
1. **Still true.** It is mechanical, it found 10 real lines today, and it needs the false-positive rules above.
2. **Memory files.** Direct edits are already authorised. Today that is one stale path, and it extends to dated faults.
3. **Followed and useful.** Store one signature per line (regex over `tools.uses`) and report the break rate and corrected rate. Add a hook first for any line broken in more than 5% of turns, since a hook-backed line is at 2.5%.
4. **Belongs lower.** Use the size and section bytes from check 1's run. Start with Repo tools (3.8 KB) and mbx (2.5 KB).

## Measure

- **Stale lines per project per machine**, weekly: 10 today for temet-proxy on omit, target 0 within one run of the pass.
- **Always-on injected bytes per project**, weekly in the learning audit: 38,280 today. It should not rise; moving the two blocks above takes it to about 31,900.
- **Corrected rate on touched lines**: for each line the pass changes, the share of turns breaking it and their corrected rate over the 14 days before and after. Baselines today: file-changes 16.1% / 48.5%, polling 8.4% / 34.0%, against a 44.6% corrected baseline.
