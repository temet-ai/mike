---
name: depend
description: Record that a work thread in mike depends on other threads, or set its branch or worktree. Use when work turns out to be blocked by, or to continue, another thread.
argument-hint: "<slug> <dependency-slug>..."
---

Change a work thread's dependencies in mike.

1. The first word of `$ARGUMENTS` is the thread; the rest are the threads it depends on. A
   thread never depends on itself.
2. Call `mcp__mike__work_thread_get` for the thread, and for each dependency to check it exists.
   If one does not, call `mcp__mike__work_thread_list` and use a slug only when the match is
   unambiguous; otherwise ask.
3. Call `mcp__mike__work_thread_checkpoint` with `depends_on` set to the existing list plus the
   new slugs (the list is replaced whole), the current state fields sent back unchanged
   (`tldr`, `where_we_are`, `next_up`, `blockers`, `open_questions`, `decisions`, `files`),
   and `branch` or `worktree` if those were given.
4. Report the dependency list and each dependency's status; one that is not `done` blocks the
   thread.
