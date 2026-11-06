# Keep work across sessions

A work thread is a note about one piece of work that is bigger than one Claude Code session.
It holds what the work is for, where it stands, what happens next, what blocks it and which
files the next session should read. mike stores it, so a new session on any machine that
shares the project can pick the work up without you retyping the story.

A thread is not a message between sessions (that is mail; see [Sessions](sessions.md)), and
it does not run anything: it records. mike has no page for threads in the web UI. You write
and read them through Claude, with the [MCP tools](mcp.md) or the `/thread:` commands below,
and find them again with project search.

Threads are scoped to a project. Nothing in a thread is sent anywhere by mike itself; a
session that reads one sends its text to Anthropic like any other conversation text. See
[Privacy](privacy.md).

## What you need

Threads work through the `mike` MCP server, so register it first: run `mike claude-code mcp`
and start a new session; see [MCP](mcp.md). The three tools that matter, `work_thread_list`,
`work_thread_get` and `work_thread_checkpoint`, are in the default tool set. A fourth,
`work_thread_add_entry`, is only in the full set but Claude can still call it by name.

For slash commands, install the mods that mike ships (`mike claude-code mods`; see
[Mods](mods.md)). One of them, `thread`, adds `/thread:list`, `/thread:get`, `/thread:new`,
`/thread:checkpoint` and `/thread:depend`.

## Start a thread on the example work

Continue the fictional `docs` session in `~/projects/site`, goal "Rewrite the install page".
You corrected Claude to put Homebrew first, and you want the work to survive a break.

1. In the session, type `/thread:new install-page Rewrite the install page, Homebrew first`.
   Without the mod, ask: "Create a work thread called `install-page` for rewriting the
   install page with Homebrew first."
2. Claude checks that the slug is free, then calls `work_thread_checkpoint` with a title, a
   goal and a short summary. A slug is letters, digits, `.`, `_` and `-`.
3. Claude reports the slug and that the thread was created.

Later, from a different session in `~/projects/site`, type `/thread:get install-page`, or ask
"Where did we get to on install-page?". Claude calls `work_thread_get` and shows the title,
status, summary, where it stands, next steps, blockers, open questions, decisions, files, the
latest history and the sessions that worked on it. The Homebrew decision is in it if the
earlier session wrote it down as a decision; threads keep what is recorded, not the whole
conversation.

## Record progress

Ask Claude to checkpoint before you stop: `/thread:checkpoint install-page`, or "Checkpoint
the install-page thread." Claude reads the thread first, then writes the state again.

A checkpoint replaces the thread's current state, so Claude sends every field again,
rewritten for now:

| Field | What it holds |
|---|---|
| summary (`tldr`) | At most 600 characters: what the next session must know first. Required. |
| where we are, next, blockers, open questions, decisions | The rest of the state |
| files | Paths, relative to the project, the next session should read |
| status | `active`, `implementing`, `review`, `parked` or `done` |
| goal, branch, worktree, acceptance criteria | Sent only when they change |

The state a checkpoint replaces is kept in the thread's history, so nothing is lost. A
`review` status is refused until the thread has acceptance criteria; write them as what is
true of the finished result, not as steps. To leave a decision, finding or hand-over note
without changing the state, Claude uses `work_thread_add_entry` with a kind of `decision`,
`finding`, `brief`, `handoff`, `note` or `checkpoint`. Entries are never edited.

## Make one thread wait on another

`/thread:depend install-page site-nav` records that `install-page` depends on `site-nav`.
Claude checks that both exist, then checkpoints with the whole dependency list. When a
dependency is not `done`, `/thread:get` says the thread is blocked on it.

## Have open threads offered at the start of a session

With the `thread` mod loaded, the first prompt of each session, and the first after a
compaction, carries the project's open threads (`active`, `implementing` and `review`) as one
line each, at most 2 KB, with a count of any left out. That text goes to Anthropic with your
prompt. Typing `#` and part of a slug offers the project's threads. Once the session reads or
writes a thread, the status line names it and when this session last checkpointed it, for
example `thread install-page · checkpointed 12m ago`. `/thread-board` opens a Threads pane of
the project's open threads grouped by status, with a switch to show parked and done ones.

## Bring in old worklog files

If you kept threads as `worklog.md` files, `mike thread import` reads them into mike:

```sh
mike thread import --dry-run --repo ~/projects/site
```

It looks in each repository's `artifacts/threads/<slug>/worklog.md` and
`docs/threads/<slug>/worklog.md`, including worktrees under `.claude/worktrees`. `--dry-run`
changes nothing and reports what it would create or update; leave it off to write. `--repo`
can be repeated, and `--registry <file>` points at a list of threads (one JSON object per
line) instead of the default `~/.claude/threads/registry.jsonl`. The report lists threads
created and updated, parse errors, skipped repositories, threads left alone and
dependencies that name no known thread. A repository that is not yet a known project is skipped and named in the report; register it
first (Claude can do this with `register_project_location`).

## If you use tether

If your sessions also load the tether mod, tether provides threads itself: the `thread` mod's
list is left to tether and stays out of the first prompt. The mods README marks the `thread`
mod as superseded by tether wherever tether runs, kept for machines without it. A public
install does not include tether, so the `thread` mod above is what you get.

## When something looks wrong

| What you see | Next step |
|---|---|
| `/thread:new` is not found | The `thread` mod is not loaded. Run `mike claude-code mods`, then start a new session. Or ask Claude in plain words; the tools work without the mod. |
| Claude has no `work_thread_*` tools | The `mike` MCP server is not registered. See [MCP](mcp.md). |
| The list is empty | The project has no threads yet. Start one as above. |
| A checkpoint to `review` is refused | Add acceptance criteria, then try again. |
| `mike thread import` says it needs a database URL | Set it in `/settings` or `MIKE_DATABASE_URL`; see [Settings](settings.md). |
| `mike thread import` says no registry and no `--repo` | Pass `--repo <path>` or `--registry <file>`. |
