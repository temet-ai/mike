---
name: list
description: List this project's work threads from mike (slug, status, title, tldr, next step). Use for "what threads exist", "which thread should I continue", or before /thread:new.
argument-hint: "[all|active|implementing|review|parked|done]"
---

List the project's work threads. A work thread is held in mike's database, not in a
`worklog.md` file, and every machine with a location for the project sees the same threads.
It is not a comms thread (`read_thread`).

1. Status: `$ARGUMENTS` if given (`all` means no filter); otherwise `active,implementing,review`.
2. Call `mcp__mike__work_thread_list` with `status` (omit it for `all`). `project` defaults to
   this session's project.
3. Show a table: slug, status, title, tldr, next step, updated. Say how many there are in
   total when `total` is larger than the rows shown, and page with `offset` when asked.
4. A row with a `parse_error` came from a worklog the importer could not read; show the error.
