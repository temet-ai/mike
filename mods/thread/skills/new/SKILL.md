---
name: new
description: Start a work thread in mike for work that spans sessions or machines. Use instead of creating artifacts/threads/<slug>/worklog.md.
argument-hint: "<slug> [what it is for]"
---

Create a work thread in mike. Never write a `worklog.md` file.

1. Slug: the first word of `$ARGUMENTS`; letters, digits, `.`, `_` and `-`. The rest is what
   the thread is for. Ask for a slug if none was given.
2. Call `mcp__mike__work_thread_list` with status `all` omitted and check the slug is free. If
   the request names work this depends on ("after", "blocked by", "continues"), find that
   thread's slug there; add it only when the match is unambiguous, otherwise ask.
3. Call `mcp__mike__work_thread_checkpoint` with `slug`, `title`, `goal`, and a `tldr` of at
   most 600 characters saying what a session picking this up must know. Add `where_we_are`,
   `next_up`, `branch`, `worktree` (relative to the project root), `depends_on` and `files`
   when known, and `contribution`: one line on what this session did.
4. Report the slug and that the thread was created (`created: true`).
