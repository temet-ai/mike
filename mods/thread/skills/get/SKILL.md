---
name: get
description: Show one work thread from mike in full - its current state, acceptance criteria, dependencies, latest history (checkpoints, briefs, handoffs) and the sessions that worked on it. Use when resuming a thread.
argument-hint: "<slug>"
---

Show one work thread from mike. It is the record that replaces `worklog.md`.

1. If no slug was given, call `mcp__mike__work_thread_list` and ask which thread.
2. Call `mcp__mike__work_thread_get` with `slug: "$ARGUMENTS"`.
3. Show the identity (title, status, goal, branch, worktree), the whole current state (tldr,
   where we are, next, blockers, open questions, decisions, files), the acceptance criteria,
   `depends_on` and `depended_on_by`, the newest history entries with their kind and body, and
   the linked sessions. Paths in `files` and `worktree` are relative to the project root.
4. When a dependency is not `done`, say so before work continues: the thread is blocked on it.
   A `brief` or `handoff` entry is what the previous session left for this one; read it whole.
5. Older history: call again with `history_before` set to `history_next_before_ord`.
