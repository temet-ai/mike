---
title: The change record
status: active
created: 2026-10-05
updated: 2026-10-06
summary: One record of what happened in a checkout, with who made each change and the content before it, the four answers a session reads from it, the lines added to answers it already gets and to the start of a turn, the two toggles that switch it on, and what it does not see.
---

# The change record

## What it lets a session do

A session working in a checkout can ask, in one call, **what changed since it last looked**,
**where things stand**, **whose a file is** and **what it has changed itself and not yet
committed**, without running `git status`, `git log` or `git diff`. Sessions ran git to learn this 5,162 times in one week. In a checkout shared by
several sessions git answers a different question from the one asked, because one working tree
and one index hold everybody's changes together. The record knows which session made each
change.

A session can also stop asking. With the second toggle on, a few lines of "since you last
looked" are added to answers a seat already gets and to the start of each turn, so what a seat
would have to remember to ask for arrives with its next answer.

The study behind this is
`artifacts/2026-10-05/comms-and-fleets/state-and-changes.md`.

## Both toggles are off

Nothing here runs until it is switched on at `/settings` (or
`PATCH /api/settings/features/<name>`); both ship off, so a deploy changes nothing.

| Toggle | When on |
|---|---|
| `change_record` | The watcher records file changes, commits and pushes, and the note, mail, check and brief handlers write an event. The three answers read what was recorded. |
| `change_record_tail` | The answers of `fleet_commit`, `fleet_check`, `write_session_note` and `send_session_message` carry `since_you_last_looked`, and the first request of a turn carries a reminder with the same lines. With it off they are byte for byte what they were. |

With both off mike behaves as it did before the record existed. Nothing in the record is on the
path that stores a request, a response or a tool call as it passes: the record is fed by its own
workers and by the handlers that already write a note, a mail, a commit or a check.

## What is in the record

One row for each event, in `workspace.change_events` (columns in
[`SCHEMA.md`](SCHEMA.md#workspacechange_events)). Each row says when, what kind, which
checkout, which session and fleet task made it when that is known, which paths it concerns, a
one-line subject, and a detail payload. A change to a file also carries the id of the content
before and after, so the state before a change can be shown later, committed or not.

| Kind | What it is | Written by |
|---|---|---|
| `edit` | A file changed by an Edit, Write or MultiEdit call | The watcher, which names the author from the tool call |
| `shell_change` | A file changed some other way, usually from a shell command | The watcher |
| `commit` | A new commit on the branch, with its paths, sha and subject | The watcher, from the branch refs |
| `push` | A remote ref moved | The watcher, from the branch refs |
| `check` | A check or the gate finished, with the commit, the verdict and the failing files | The handlers in `launch/fleet/check_report.rs` and `gate_queue.rs` |
| `verdict` | A note whose data carries a `verdict` | The note handler |
| `note` | Any other session note | The note handler |
| `mail` | A message between sessions | The mailbox handlers |
| `brief` | A brief note written or replaced | The briefs-as-notes writer (see [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md)) |
| `task_state` | A fleet task changed state | Reserved in the schema; no writer yet |

### The watcher

For every checkout in which a session made a request in the last 30 minutes, mike runs a
watcher. It asks which checkouts are being worked in every 30 seconds, starts one watcher for
each, and stops one when nobody has worked there for 30 minutes. It is not limited to fleets.

- **File notifications.** The operating system tells the watcher a file changed (inotify here,
  FSEvents on the Mac). It waits about 300 ms to collect a burst, leaves out what git ignores
  and the `.git` folder's objects, and gives each changed path a blob id with
  `git hash-object -w`, which also stores the content.
- **Who.** The event is recorded at once with no author. A few seconds later the author is
  named: the session whose Edit, Write or MultiEdit call named that path in the half minute
  before, but only when no other session had an edit call or a shell command running then;
  failing that the one session that had a shell command running; failing that nobody, with the
  candidates listed in the event's detail.
- **Commits and pushes.** The watcher also watches the branch refs. A new commit is one event
  with the commit's paths, and its author comes from the commit's `Fleet-Seat` trailer or else
  the session whose shell ran `git commit`. A moved remote ref is one `push` event.
- **The backstop.** Notifications can be missed. Every 30 seconds the watcher compares one
  `git status --porcelain=v2 -z` with what it knows and records the difference. When
  notifications cannot start at all, it makes the same comparison every 2 seconds.
- **The fleet's own watcher.** With `change_record` on and a watcher running for the run's
  checkout, `launch/fleet/watch.rs` takes the fleet's tree changes from this feed instead of its
  own once-a-second `git status`, and writes the same `launch.tree_changes` rows. With the
  toggle off it is unchanged, so `list_fleet_changes` and `list_fleet_results` return what they
  always did.

### Notes, mail and checks

A note, a mail and a finished check each become one event whose paths are the files concerned.
Paths are worked out, never guessed: a note's `data.paths` and a mail's context items of kind
file and commit come first; a commit id in the text gives that commit's paths; a word with a
slash that is a tracked path in the checkout counts. When no path can be established the event
is written with fewer paths rather than invented ones. The writing happens after the answer has
gone out, so a note or a mail is never slower, and never fails, because of it.

## The four answers

One route, `GET /api/changes?view=changes|standing|whose|mine` (and `tail`, below), and one MCP tool, `what_changed`
(see [`MCP_BRIDGE.md`](MCP_BRIDGE.md)), in the default core profile.

- **`changes`**: what others changed since the caller's place (or since a named commit or time),
  for the caller's own paths, for the paths it has read, for named paths, or for everything.
  Each line says who, when, how (an edit, from the shell, a commit), the commit when
  committed, and the notes, mail and check verdicts on those paths. Asking moves the place.
- **`standing`**: for a fleet seat, its task and state, its own uncommitted paths, the head
  against the remote (how far ahead and behind), the latest check, and how many events wait past
  its place (counted up to 200). For a
  session outside a fleet, the other sessions working in the checkout with the paths each has
  uncommitted, and the five newest commits with who made each.
- **`whose`**: for a path, the task that owns it in a running fleet, who last changed it, and
  whether that change is committed.
- **`mine`**: what the caller itself changed and has not committed. Its own paths (an edit or a
  shell change by this session newer than the newest commit that names the path), each with the
  time of its last change; whether the project's check ran at a tree state after the caller's
  last change, and its verdict (the fleet's declared check, else the newest check recorded for
  the checkout); and the check and verdict events attached to those paths. It does not move the
  place. `fleet_commit` with no paths named commits these paths when the record is on (the answer
  says `paths_from: record`), and the task's files when it is off or has nothing for the session
  (`paths_from: task`); `fleet_report` uses the same check-after-last-change answer.

### Each session's place

`workspace.change_places` holds, for each session and checkout, the id of the last event the
session was shown. "Since I last looked" is a range after that id in an indexed table. `changes`
and the lines below move the place; a second session asking gets its own rows, and asking again
gets none until something new is recorded.

### The lines added to other answers

With `change_record_tail` on, the answers of `fleet_commit`, `fleet_check`, `write_session_note`
and `send_session_message` carry `since_you_last_looked`: at most twelve short lines of what
`changes` would say for the session's own paths (eleven changes, then one line saying more wait
when there are more), and nothing when nothing changed. A session's first look records its place
and adds no lines.

The first request of a turn of a session that has a git checkout carries the same lines as a
reminder headed `What others changed since you last looked:`, added to the end of the messages,
never to the system prompt. It is added once per turn, not on later requests of the turn, never
to a subagent's request, and not when there are no lines. A seat shown the lines by the reminder
is not shown them again by its next note or commit, because showing them moves the place. With
the toggle off the request is byte for byte what it was.

`GET /api/changes?view=tail&session=<id>&checkout=<root>` returns the same lines as
`{lines, tail: true}` and moves the place the same way, for a session's own tooling to show them
itself. Because the reminder and this view share the session's place, whichever shows the lines first
leaves nothing for the other, so a session that has both is shown each change once. With
`change_record_tail` off it returns `{lines: [], tail: false}` and moves nothing.

`GET /api/changes?view=feed&checkout=<root>` returns the 50 newest events in the checkout, newest
first and every author included (the caller's own, other sessions', and changes with no author
known, such as the person's own editor or shell). It never moves a place, so a pane can poll it
without taking lines from the reminder or `tail`. tether's `/changes` draws it in a pane
as a tree of the changed files, each with who last changed it, how, whether it is committed and
how long ago; picking a file shows its recorded history (`/changes` opens the pane).

A check or gate event names no session; its subject says which check, whether it passed and
at which commit, and its paths are the failing files.

## What is not recorded

- A change made by something that does not pass through mike (the operator's editor, a
  background job) is seen by the backstop and has no author.
- When two sessions' shell commands overlap, a change between them cannot be given to one; the
  event has no author and lists the candidates.
- A shell command with no captured result counts as still running only while its session is
  alive (it made a request after the call began and within five minutes of the change) or the
  call is younger than its own timeout (the call's timeout, else 120 seconds, at most 600
  seconds). A session that died in the middle of a command is therefore not named as the author
  of a later change.
- An edit proven by the file's content is still left without an author, with both sessions
  listed, when another session's shell ran in the same window and the content after the change
  is not exactly what the edit alone would give.
- A file change left without an author is named again once, 30 seconds later, so an edit row
  stored late still names its seat.
- Nothing before the toggle was switched on: the record starts empty.
- The record is a rolling account of work, not a copy of the repository. It keeps the ids of
  contents git stored; it does not replace git.
