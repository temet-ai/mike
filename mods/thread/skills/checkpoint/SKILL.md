---
name: checkpoint
description: Record where a work thread stands in mike so the next session, on any machine, can resume it. Use before ending a session on thread work, after a milestone, or when handing off. Replaces editing worklog.md.
argument-hint: "[slug] [one-line summary]"
---

Checkpoint a work thread in mike. Never edit a `worklog.md` file.

## Status

| Status | Meaning |
|---|---|
| `active` | In progress, no acceptance bar yet |
| `implementing` | Building something specific; needs acceptance criteria |
| `review` | Criteria met and seen working for real; mike refuses it without criteria |
| `parked` | Paused |
| `done` | Complete |

A criterion says what is true of the running result, not what to do
(`.claude/rules/workflow-and-documentation.md` in mike's repository).

## Steps

1. Find the thread: the slug in `$ARGUMENTS`, else the one this session has been working on,
   else call `mcp__mike__work_thread_list` and pick the only plausible one or ask.
2. Call `mcp__mike__work_thread_get` to read the current state, dependencies and criteria.
3. Call `mcp__mike__work_thread_checkpoint` once. The state fields replace the current state
   whole, so send each one again, rewritten for now:
   - `tldr`: at most 600 characters, what the next session must know first.
   - `where_we_are`, `next_up` (numbered steps), `blockers`, `open_questions`, `decisions`.
   - `files`: paths relative to the project root that the next session must read.
   - `contribution`: one line on what this session did. `commit_sha`: `git rev-parse HEAD`.
   - `status`, `branch`, `worktree`, `depends_on` (the whole list) and `acceptance_criteria`
     only when they change.
   The replaced state is kept in the thread's history, so nothing is lost.
4. A handoff or brief for a specific next session: also call `mcp__mike__work_thread_add_entry`
   with `kind: "handoff"` or `"brief"` and the full text as `body`.
5. Report the slug, the new status and `entry_ord`.
