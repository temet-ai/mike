---
title: Launch Domain
status: active
created: 2026-09-17
updated: 2026-10-22
summary: mike's own terminal launcher — the TerminalBackend trait and its adapters, the mike spawn CLI verbs, the spawn_session/list_launches/kill_session MCP tools, and how a launch's lifecycle is observed.
---

# Launch Domain

`crates/mike/src/launch/` opens a terminal surface in a folder and starts a fresh Claude Code
session in it, wired to the proxy, with a preassigned session id — mike's own replacement for a
standalone terminal-launcher process. It is reached three ways: the `mike spawn` CLI verbs, the
`/api/launches` inspect routes, and the `spawn_session`/`list_launches`/`kill_session` MCP tools
(documented in [`MCP_BRIDGE.md`](MCP_BRIDGE.md)), which are HTTP clients of those routes.

These do not share one process or one `AppState`. `mike spawn` runs the launch in its own
process: it loads its own `ProxyConfig` (`crates/mike/src/launch/cli/mod.rs`) and builds its own
`Runner` over the backend, the ledger and the trust gate
(`crates/mike/src/launch/cli/runner.rs`), never calling the inspect API. The API path runs the
same launch code inside the running `mike` server, entered through
`open_for_server`/`kill_for_server`/`doctor_for_server` (`crates/mike/src/launch/cli/server.rs`),
with the server's own `AppState`. What all of them share is the `launch.*` ledger in Postgres —
which is why a launch made through any entry point shows up in the others.

## 1. Terminal backends

Every backend implements `TerminalBackend` (`crates/mike/src/launch/backend/mod.rs`):

```rust
trait TerminalBackend: Send + Sync {
    fn id(&self) -> BackendId;
    fn capabilities(&self) -> Capabilities;
    async fn probe(&self) -> ProbeReport;
    async fn validate(&self) -> ValidationReport;
    async fn open(&self, req: &OpenRequest) -> Result<Surface, LaunchError>;
    async fn find_by_session(&self, session: Uuid) -> Result<Option<Surface>, LaunchError>;
    async fn close(&self, surface: &Surface, wait: Duration) -> Result<CloseOutcome, LaunchError>;
    async fn list(&self) -> Result<Vec<Surface>, LaunchError>;
}
```

`Capabilities` declares what a backend can actually do — `placement_workspace`/`placement_tab`/
`placement_split`, `agent_state`, `prompt_in_argv`, `close_by_session`, `events` — so a caller
checks a capability before relying on it rather than discovering the gap at runtime. A backend
without `agent_state` cannot tell you whether a launched session is idle, working or blocked;
liveness for those backends comes from the workspace roster's channel fields alone (the
workspace-coordination tools in [`MCP_BRIDGE.md`](MCP_BRIDGE.md)), not from the backend.

`backend_for(BackendId)` hands out a backend. Five ids have a real adapter today
(asserted by `backend_for_herdr_tmux_terminal_and_none_exist_ghostty_absent` in
`crates/mike/src/launch/backend/mod.rs`):

| Backend | Module | What it is |
|---|---|---|
| `herdr` | `crates/mike/src/launch/herdr/` | Drives the `herdr` CLI (JSON on stdout, one process per call, bounded timeouts, never a shell) against its persistent server. The only backend with `agent_state` and a session-uuid join, so it gives the richest liveness reads. |
| `herdr-core` | `crates/mike/src/launch/herdr_core/` | The Herdr desktop app. Starts claude with `hc run --bg --keep` labelled `herdr.space`/`herdr.tab`, which the app adopts as a tab: in the caller's space when it is in the folder, else a space already in the folder, else a new space (a fleet run gets one space). `mike.session`/`mike.name`/`mike.run`/`mike.seat` labels drive find, list and kill (`hc kill`); the first prompt goes by `hc send`, confirmed when the agent reports working or its turn advances. herdr-core carries Claude Code's working/idle/blocked/done, so it has `agent_state`. Fleet focus uses `herdr-app do agent.focus`. |
| `tmux` | `crates/mike/src/launch/tmux/` | Drives the `tmux` CLI for headless hosts — SSH sessions, containers, anything with no display for herdr's server. No `agent_state`. |
| `terminal` | `crates/mike/src/launch/terminal/` | **Hyprland only.** A plain window per launch via `xdg-terminal-exec`, found again through `hyprctl clients -j` and closed with a direct signal on the window's pid — no persistent server and no multiplexer pane, so every launch is its own OS process tagged by an `--app-id`. `Capabilities::prompt_in_argv` is `true`: there is no side channel to submit a prompt after the window opens, so the prompt travels in the claude argv. |
| `none` | `crates/mike/src/launch/backend/none.rs` | **The default.** Hand-launch only, and it means "no backend chosen yet", not "launching is disabled". A real implementation so the domain never special-cases "no backend"; every launch verb refuses with the remedy ("`mike claude` in the current pane; `mike spawn open` is unavailable"), and every read answers "nothing". |

The `terminal` backend's probe (`crates/mike/src/launch/terminal/probe.rs`) requires all three of
`xdg-terminal-exec` on `PATH`, a `hyprctl version -j` that answers, and a non-empty
`$WAYLAND_DISPLAY` — it reports unavailable if any one fails, so it is not a generic
Wayland or X11 backend. On a host without Hyprland, use `herdr` or `tmux`.

`ghostty` is a recognized `BackendId` (it appears in the schema's CHECK constraint and the setup
wizard's backend list) with no adapter yet — it parses, but `backend_for(BackendId::Ghostty)`
returns `None`, and configuring it makes every `mike spawn` verb fail with
`BackendUnavailable`.

A launch opens where its caller runs (`crates/mike/src/launch/backend_choice.rs`, #691). A caller
in a Herdr app terminal (`HC_TERMINAL_ID`) gets `herdr-core`; one in an old herdr pane
(`HERDR_PANE_ID`) gets `herdr`; one in tmux gets `tmux`. `spawn_session` forwards those variables
as `caller_env`. With no caller to go by (a fleet seat, a phone over Remote Control), the first of
`herdr-core`, `herdr`, `tmux` whose socket answers is used, else `launch_backend` /
`MIKE_LAUNCH_BACKEND` (default `none`, set by the setup wizard's terminal step or directly).
`launch_backend_follow_caller = false` always uses `launch_backend`, and a `POST /api/launches`
body may name a `backend`. The choice and its reason are recorded on
`launch.launches.backend_reason`, returned by the launch API and printed by the CLI; kill reuses
the backend a launch recorded. While `launch_backend` is `none` nothing is chosen and every launch
verb refuses. `mike spawn doctor`
re-runs the configured backend's probe and, when it is available, a live validation in a scratch
workspace; every validation is recorded on `launch.backend_validations` (see
[`SCHEMA.md`](SCHEMA.md#domain-launch)) so setup, `/settings` and doctor all read the same "when
did this backend last actually work" history.

## 2. The `mike spawn` CLI

Eight verbs, all in `crates/mike/src/launch/cli/`:

| Verb | Does |
|---|---|
| `open <folder> [placement] [flags] [claude args]` | Opens a terminal surface in `folder` and starts `claude` in it with a preassigned session id. |
| `kill --session <uuid>` | Closes the surface hosting `<uuid>`, and only that one; a second call is an exit-0 no-op. |
| `list [--run <id>]` | One line per surface the backend hosts, joined against the ledger's open launches, newest launch first; unmatched surfaces follow in surface-ref order. `--run` scopes to one run's seats. |
| `doctor` | Runs every dependency check for a launch (backend, sidecar, CA certificate, the `claude` binary, the inspect API, the database) with the remedy beside each failure. |
| `warm <folder> --name <discipline> [flags] [claude args]` | Launches exactly like `open`, then records the claude arguments as a named template; a second `warm` under the same name supersedes the previous one. |
| `fork <discipline\|session-uuid> [--folder <dir>] [flags] [claude args]` | Resolves a template by name or session uuid, then launches through the same path as `open` with `--resume <template session> --fork-session --session-id <new>` and the template's recorded argv prepended ahead of the caller's own. |
| `templates [list\|show <name>\|delete <name> --yes]` | Lists, inspects or closes warm templates on record. |
| `reap [--yes]` | Lists (dry run by default) orphaned claude processes — never the caller's own process or any of its ancestors — and, with `--yes`, signals them. |

`open`, `warm` and `fork` share one argument shape: the folder (or, for `fork`, the template)
comes first, spawn's own flags may appear anywhere after it, and every other token is a claude
argument kept in order; a literal `--` forwards the rest verbatim. The shared flags are parsed by
`split_launch_args` (`crates/mike/src/launch/request/split.rs`); the rest are verb-specific.

| Flag | Verbs | What it does |
|---|---|---|
| `--tab` | open, warm, fork | Tab placement — what a launch gets when no placement flag is given. Mutually exclusive with `--split`. |
| `--split <l\|r\|u\|d>` | open, warm, fork | Split placement in that direction. A backend without split support ignores it and reports what it actually did in the launch's `actual_placement`. |
| `--panel` | open, warm, fork | Panel placement. Unlike `--split`, it may be combined with `--tab`. |
| `--anchor-terminal <surface ref>` | open, warm, fork | Targets a specific existing pane for a split placement. Refused (`Unsupported`) by a backend with no addressable-pane concept to anchor against. |
| `--run <id>` | open, warm, fork | Groups the launch with its run's siblings. On herdr, when no placement flag is given, the first launch under a run id creates a workspace labelled with the run id and records it on `launch.runs`; every later launch under that id goes into that workspace. The seat's role picks its tab: coordinator, observer and acceptance go to `control`, a seat named `template…` to `templates`, everything else to `workers`. Inside a tab the largest pane is split, to the right when it is at least twice as wide as it is tall and down otherwise, and never below 100 columns by 30 rows; a tab with no room starts `workers 2`. The pane is named after the seat and the seat name is reported to herdr for its sidebar (`crates/mike/src/launch/herdr/run_layout.rs`). An explicit `--tab`, `--split` or `--panel` places the launch the old way, by folder. tmux and terminal do not group by run. |
| `--seat <name>` | open, warm, fork | Names this launch's seat within its run, recorded on the ledger row alongside the run id. Also becomes the session's roster **name** unless `--session-name` overrides it. |
| `--role <role>` | open, warm, fork | The role the new session declares — free text, open set, and what `role:` mailbox fan-out resolves against. On `warm`, defaults to the template's `--name <discipline>`. |
| `--instructions <text>` | open, warm, fork | The standing brief the new session works under, distinct from its goal and from the first-turn prompt. |
| `--goal <text>` | open, warm, fork | What the new session is working on. |
| `--session-name <name>` | open, warm, fork | The roster name, when it should differ from `--seat`. Note `--name` is **not** this: on `warm` it already means the template's discipline. |
| `--prompt-file <path\|->` | open, warm, fork | Reads the brief from a file, or from stdin with `-`. There is no inline prompt flag. |
| `--worktree-ok` | open, warm, fork | Permits launching inside a linked git worktree that sits outside the repository's `.claude/worktrees/`, which is otherwise refused; a worktree under `.claude/worktrees/` needs no flag. |
| `--no-carry-config` | open, warm, fork | Skips copying the main checkout's carried config files into a linked worktree the launch accepts. |
| `--trust` | open, warm, fork | Records the folder in `~/.claude.json` before launching, so Claude Code's own trust dialog cannot swallow the prompt. |
| `--no-trust` | open, warm, fork | Refuses an untrusted folder instead of trusting it — overrides `launch_auto_trust`. |
| `--no-proxy-env` | open, warm, fork | Launches without `HTTPS_PROXY`/`NO_PROXY`/`NODE_EXTRA_CA_CERTS` pointed at the forward sidecar. |
| `--no-remote-control` | open, warm, fork | Launches without appending `--remote-control` to the claude argv. |
| `--no-channels` | open, warm, fork | Launches without adding `--dangerously-load-development-channels server:mike`, opting out of the default MCP channel wake path. |
| `--json` | every verb | Prints exactly one JSON object on stdout instead of the human rendering. |
| `--name <discipline>` | warm | Required. The template name this launch is recorded under. |
| `--folder <dir>` | fork | The folder to launch the fork in, when it differs from the template's. |
| `--allow-prefix-change` | fork | Opts into a prefix-flag divergence from the template (see below). |
| `--session <uuid>` | kill | Required. The session whose surface to close. |
| `--run <id>` | list | Scopes the listing to one run's seats. |
| `--yes` | reap, `templates delete` | Confirms the destructive action; without it `reap` is a dry run. |

The legacy `-m <model>` and `-p`/`--prompt` forms are rejected with a usage error rather than
being passed through to claude.

When `open`, `warm`, `fork` or `spawn_session` names a recorded `fleet-<slug>-<8 hex digits>`
run and supplies a role or seat, it inherits the fleet's effective seat settings before opening:
scratch directories, config isolation, plugin defaults, MCP profile and the definition's
`seat_settings` overrides. The explicit role selects the profile; otherwise the seat name does,
including numbered control successors such as `acceptance-2`. The lookup verifies the exact
recorded run id and is bounded to five seconds. Missing, mismatched or unavailable fleet records
refuse the launch with `fleet_settings`, rather than opening a partially configured successor.

An existing Claude `--settings` or `--settings=...` argument is a complete override: mike keeps
it verbatim and skips automatic inheritance and its lookup. This applies to caller arguments,
template arguments and operator launch defaults. To inherit fleet defaults plus a custom
override, put the custom fields in the fleet definition's `seat_settings`; the explicit
`--settings` path does not merge them. Ordinary runs and detached `retro-*` runs are unchanged.

`fork` refuses when a caller-given `--model`, `--agent`, `--append-system-prompt-file`,
`--settings`, `--add-dir` or `--mcp-config` disagrees with what the template recorded for it —
these six flags fix a template's prompt-cache prefix, and a fork that silently diverged from them
would pay a full cache miss on its first request without saying why. `--allow-prefix-change` opts
into the divergence explicitly. The forked session's own id is always fresh, never the template's.

### Spawning a session that is already somebody

`--role`, `--instructions`, `--goal` and `--session-name` are the session's **identity**, and they
are applied without the session doing anything. Because the launcher preassigns the session uuid
(see below), mike records the identity against that uuid on the ledger row
(`launch.launches.identity`, a JSONB handoff payload). When the session starts, the launcher's
registration (or, failing that, mike's passive registration on its first captured request)
reads the pending identity and writes it to `sessions.session_goals`. The seat is on the roster as `observer-1 [observer]`, with its charter
readable by peers, **before its first turn and without a single tool call**.

Precedence is strict: a value passed explicitly on the registration call wins; otherwise the
launch-seeded value applies; otherwise whatever the session already has is left alone. A seed never
overwrites an identity a session declared for itself — SessionStart re-registers on every start, so
a stale seed clobbering a mid-run self-declared role is exactly the failure this ordering prevents.

A seat launched without these is a uuid prefix with no role, which means `role:` mail never reaches
it and a later "what did the observers find" query does not see it. `spawn_session` over MCP takes
the same fields under the plain names `name`, `role`, `instructions` and `goal`.

### Session id and brief delivery

`--session-id <uuid>` after `--` preassigns the launch's session id; the launcher pushes it first
in the claude argv it hands to the backend. Without it, a fresh launch gets a freshly generated
uuid. `--resume <uuid>` keeps that uuid as the launch's session id (nothing is preassigned: the
resumed session already has it). `--continue`, `--resume` with no uuid, and `--resume … --fork-session`
without `--session-id` leave the session id unknown, because claude picks or mints it. The literal placeholder `{{session_id}}`, wherever it appears
in the prompt or in any claude argument, is substituted with the launch's real session id — known
or generated — before the surface opens, so a brief can state a seat's own identity without the
caller minting a uuid and threading it through by hand.

There is no inline prompt flag. `--prompt-file <path|->` reads the brief from a file, or from
stdin with `-`. Once mike has the text, `launch_brief_inline_max` (default 800 characters,
`MIKE_LAUNCH_BRIEF_INLINE_MAX`) decides how it reaches the surface: at or under the limit it
travels with the claude invocation itself (or however the backend submits a prompt, when
`Capabilities::prompt_in_argv` is false). The launch row (`launch.launches.brief_text`) always
carries the whole text, never a slice.

**Briefs are session notes.** When the session id is known and the ledger is reachable, the
launcher mints a note uuid (`launch.launches.brief_note_uuid`) for every brief, inline ones too.
Workspace registration turns the launch row into a `workspace.session_notes` entry under exactly
that uuid (`kind` `brief`, tags `brief`/`launch`, `data` carrying launch id, run, seat, model and
agent), so `list_session_notes` with `kind: "brief"` finds what any spawned session was told.

**Over the limit, the brief is pushed over Remote Control, or staged to a file.** With the
`launch_brief` feature toggle on (global, default off, `/settings`; the launcher reads it at each
spawn), the launch's `brief_mode` is `note` and nothing is typed into the session. Once the
channel check links the session's Remote Control stream, the launcher calls
`POST /api/launches/{session}/brief`. mike mails the brief from itself (no sending session) under
the note's uuid and pushes it over Remote Control. Claude Code presents that push as "Another
Claude session sent a message … treat it as a teammate's request", so the idle session starts
work on it; the brief is inserted once and stays in the session's own history. Brief mail is never
delivered through the mike channel, which Claude Code frames as untrusted data it will not act on
(see [`INJECTION_TRUST.md`](INJECTION_TRUST.md)). Claude Code's receipt of the push marks the mail
consumed, and `launch.launches` rows (`mike spawn list`, `list_launches`) carry that as
`brief_read_at`: the moment the session took its brief in. If nothing was pushed (no Remote
Control stream, no proxy, a failed call), the launcher types a short pointer instead, the
operator's own first message telling the session to read its brief note with
`list_session_notes`. With the toggle off, the session id unknown, or the launch row not written,
mike falls back to `brief_mode` `file`: it stages the text to a private 0600 file under its briefs
directory (`~/.local/state/mike/briefs/<session>.md`) and tells the session to read it.

**A fleet can keep its briefs as notes.** A fleet whose definition carries
`briefs: {"as_notes": true}` (default off; `define_fleet` accepts it and `get_fleet` shows it)
stores each brief in parts instead of as one text per seat. A note of kind `brief` tagged `part:run`
holds the run's part: the goal, the ask, the criteria, the rules, how the project is built and
checked, and the shared files. One note tagged `part:task` and `task:<id>` holds each task's part:
its name, brief, criteria and files. The discipline's template is given the run's part when it is
warmed, so every seat forked from it starts with that already in context. The file a worker reads
at launch is written from the notes: the task's part, then one line naming the run note and
saying how to read it. It does not list the other seats; a seat reads them with `get_fleet`.
With `briefs.as_notes` on, a seat's first message is composed from its run note and its task
note, each headed with the note id and version, with a "Brief parts given" line recording the
versions, and no brief file is written; if `as_notes` is off or a note cannot be read, the file
is written as before.
When the task list changes, the changed part is stored as a new note with the next version and
`supersedes` naming the old one, and the seat is mailed only the lines that differ. Each new note
also writes a `brief` event to the [change record](CHANGE_RECORD.md) while that is on. Without
the item every brief is exactly what it was.

By default a launch also gets `--remote-control`, the MCP channel flag
`--dangerously-load-development-channels server:mike`, and the proxy environment —
`HTTPS_PROXY`/`NO_PROXY`/`NODE_EXTRA_CA_CERTS` pointed at the forward sidecar. `remote_control`,
`channels` and `proxy_env` are **per-launch** fields, not boot config: on the API they are fields of the
`POST /api/launches` body defaulting to true (`CreateLaunchRequest`,
`crates/mike/src/api/launches/mod.rs`), and on the CLI they are the same three booleans carried in
`SplitLaunchArgs`, turned off per launch with `--no-remote-control`, `--no-channels` or
`--no-proxy-env`. There is
no `launch_remote_control` or `launch_proxy_env` in `LaunchConfig`. What *is* boot config is
where the environment points — `launch_proxy_forward_url`, `launch_no_proxy`, `launch_ca_cert`
— and the rest of the `launch_*` knobs in [`CONFIGURATION.md`](CONFIGURATION.md).

MCP channel delivery requires the `mike` server to be configured at user scope and Claude Code's
development-channel confirmation. The herdr adapter watches for that confirmation and accepts it
when it appears; other backends may need the operator to do so. A live mike-mcp listener takes
mailbox delivery ahead of Remote Control; without it, Remote Control push or next-request
injection remains the fallback. A launch's Remote Control `channel_check: found` does not prove
the MCP listener is live or a message has been read. See [`MCP_BRIDGE.md` §7](MCP_BRIDGE.md#7-channel-push-mailbox-wake-over-mcp).

An untrusted folder is refused unless `--trust` (or `launch_auto_trust`, default false) records it
in `~/.claude.json` first, so Claude Code's own trust dialog cannot swallow the prompt.
A linked git worktree under the repository's `.claude/worktrees/` launches as usual; one anywhere else is refused unless `--worktree-ok` is passed, because worktrees live inside their repository.

### Exit codes

`LaunchError::exit_code` (`crates/mike/src/launch/error.rs`) is the authoritative mapping:

| Code | Meaning |
|---|---|
| `0` | The verb did what it was asked. |
| `1` | Refused or failed — `Usage`, `Refused`, `Failed`, `Io`, `Timeout`, `BackendResponse`, `Malformed`, including a channel-check timeout on an otherwise-open surface. |
| `2` | `Unsupported` (the backend has no such capability, e.g. `--anchor-terminal`) or `Unconfirmed` (the action may have happened; nothing proved it). |
| `69` | `BackendNone` or `BackendUnavailable` — no backend chosen, or the configured one has no adapter / is not answering. |
| `127` | `ExecutableNotFound` — a required executable is missing (the backend CLI, or `claude` itself). |

The `after_help` text on `mike spawn open` lists only 0/1/69/127 and omits 2; the table above
matches the code.

## 3. Lifecycle and how it is observed

`POST /api/launches` (also used by MCP `spawn_session`) starts a launch in the background.
The CLI `open`, `warm` and `fork` commands wait for surface opening, prompt delivery and the
channel check before returning: `open::execute` awaits `launch`, which awaits `finish`; `warm`
also awaits `open::finish`, and `fork` calls `open::execute`. The session itself continues
running after either entry point returns.

The HTTP surface is narrower than the CLI: `crates/mike/src/api/manifest/launches.rs` registers
create (`POST /api/launches`), list (`GET /api/launches`), detail
(`GET /api/launches/{session}`), kill (`DELETE /api/launches/{session}`) and doctor
(`GET /api/launches/doctor`) — and nothing else. `warm`, `fork`, `templates` and `reap` are
CLI-only; there is no route and no MCP tool for them. `mike spawn open`/`warm`/`fork` do **not**
call `POST /api/launches`; they run the launch in-process. The route is what `spawn_session` and
the SPA call.

`POST /api/launches` returns `{launch_id, session, backend}` as soon as the ledger row exists and
the fast, local checks pass — the trust gate, the worktree refusal, the surface cap (`launch_warn_surfaces`/
`launch_max_surfaces`). Opening the surface, registering the session on the workspace roster
(`registered_via: launcher`, before the session's first request; carries `location_uuid` when the folder's repository has a `.mike/knowledge.json` and the machine has a `machine.json`, see [`PROJECT_KNOWLEDGE.md`](PROJECT_KNOWLEDGE.md)), and waiting for its Remote
Control channel all continue afterward. A synchronous refusal (bad folder, an untrusted folder
without trust, no backend configured, the surface cap reached) returns an error instead of a
`launch_id` and never reaches the ledger.

Four ways to watch a launch finish landing, in order of cost:

- **Poll `GET /api/launches`** (MCP `list_launches`, optionally `?run=`) for the
  same session and read `opened`, `delivered`, `confirmed`, `channel_check` and `result` —
  `pending` until the outcome is known, then `success`/`unconfirmed`/`failed`.
- **`GET /api/launches/{session}`** for one launch's full row.
- **Subscribe to `/api/ws`** and watch for a `RowChanged` event on `launch_launches` carrying the
  launch's `id` and `session_id` — the same `proxy.row_changed` NOTIFY channel every other
  live-updating table already uses, fired by the trigger in migration
  `20260916030000_launch_launches_row_changed_trigger.sql` on every insert, update and delete.
- **`GET /api/workspace/roster`** for the session's own row, once it should have registered.

`mike spawn list` is a surface inventory, not the same response shape as `list_launches`: it
shows the surface, folder, session, backend agent state, run and seat. Use the API or MCP tool
for `result`, `channel_check` and the delivery fields. The ledger query orders newest first
(`created_at DESC, id DESC`); the CLI preserves that order for joined surfaces.

`channel_check` records whether the launcher found the session's Remote Control channel:
`found` is the only pass; `timeout`, `db_unreachable`, `no_session_id` and `disabled` are not.
`result` on the ledger row is the terminal verdict: `pending` while still in flight, `success`
once opened/delivered/confirmed/channel-checked all landed, `unconfirmed` when the surface opened
but delivery or the channel check could not be confirmed, `failed` otherwise — with `reason` and,
for a failed backend call, a capped `stderr_text`/`stderr_truncated` pair.

`DELETE /api/launches/{session}` (`mike spawn kill` / `kill_session`) closes the surface hosting
a session, waiting up to `launch_kill_wait_secs` (default 20s) for the claude process to exit and
reporting `process_exited` in the response. A session with no known surface, or a surface already
gone, comes back as a no-op (`closed: false`) rather than an error. Closing a surface calls the
backend's own close (a herdr pane close, a tmux kill-window, a direct signal on the terminal
adapter's tracked pid) — there is no interactive window-close dialog to answer, but the claude
process under it can still take on the order of ten to twenty seconds to exit after the surface
is gone, so `list_launches`/`mike spawn list` reporting the surface closed is not by itself proof
the process has too.

## 4. Storage

The `launch` Postgres schema (`launches`, `templates`, `runs`, `backend_validations`) is the
durable record of every launch attempt, whatever its outcome, and of the warm templates a fork
starts from — see [`SCHEMA.md`](SCHEMA.md#domain-launch) for the full column reference. Nothing
here is in memory only: a restart of the `mike` binary loses no launch history, and the CLI, the
MCP tools and the inspect UI all read the same rows.

## 5. The `mike fleet` command

`mike fleet` is the operator's window onto a fleet defined in mike (`launch.fleets`). It holds no
database connection: its operator commands call the inspect API at the configured
`inspect_listen_addr`, so it works from any shell that can reach that address. `<fleet>` is a fleet
uuid or a slug, passed through to the API unchanged.

| Command | Calls | Prints |
|---|---|---|
| `mike fleet status <fleet> [--watch <seconds>] [--json]` | `GET /api/fleets/{fleet}/status` | One screen: what is missing, the header, every task, every open seat, the newest verdicts and what is blocked. `--json` prints the answer as received. |
| `mike fleet list [--project <path>] [--json]` | `GET /api/fleets?project_path=&limit=` | One line per fleet: slug, state, how many items are missing, when it was created, the run id. |
| `mike fleet start <fleet>` | `POST /api/fleets/{fleet}/start` | The state the fleet is now in. Needed only when the `fleet_auto_start` setting is off. |
| `mike fleet stop <fleet>` | `POST /api/fleets/{fleet}/stop` | The fleet's slug and `done`, then one line per open seat saying whether its pane was closed. Closes each open seat's pane (as `mike spawn kill --session` would), records each launch closed with reason `fleet stop`, closes the run and marks the fleet `done`. Refused with `409` unless the fleet is `running`. It never closes the whole workspace, which can hold panes that are not seats. |

**mike carries the fleet's files.** The role files (`fleet-coordinator`, `fleet-acceptance`, `fleet-observer`, and the off-seat `fleet-rule-auditor` and `fleet-capability-author`), the `fleet-worker` skill (installed to `~/.claude/skills/` and appended to every worker seat, whatever agent it runs as, or none), the shared rules, the observer contract, the observer's run report script and the `/fleet` command are compiled into the binary from this repository's `.claude/` (`launch/fleet/bundle.rs`). When a fleet is checked for readiness, mike writes the rules, contracts and script to `~/.local/state/mike/fleet/`, the role files to `~/.claude/agents/` (only files it wrote before, never one the operator edited) and the command to `~/.claude/commands/fleet.md` (only when nothing is there). A project's own copy under its `.claude/fleet/` or `.claude/agents/` takes precedence over mike's, file by file; briefs name the copy in use. A project says how it is built in `.claude/fleet/project.md`, which every brief names when it exists. So a fleet starts in any project mike runs, and a project that wants different seats or rules keeps its own versions.

**Fleet seats refuse accidental desktop opens by default.** On Unix, every fleet role,
warm template and inherited successor gets `BROWSER=true` plus a private helper directory.
Mike installs the helpers before launch and refuses a launch when installation fails. The
terminal backend activates that directory in the actual launch shell before Claude starts,
so MCP servers launched by the seat inherit it too. For Herdr, Mike waits for the existing
shell-prompt check, submits activation once and requires an exact private per-launch receipt
before requesting agent start. The whole activation has a 20-second deadline. A command
submission acknowledgement alone is insufficient; absent, mismatched or uncertain activation
refuses the launch. This receipt confirms this setup step, not host identity or process ownership.
A SessionStart hook also appends the
activation to Claude's environment file for subsequent Bash tools, retaining existing hooks
and environment content. The launch server's PATH is never substituted for the shell's PATH.

PATH-resolved `xdg-open`, `open`, `gio open` and `gio launch` return exit 126 with an explicit
blocked diagnostic. They do not report a successful browser opening. Other `gio` operations
resolve the original executable after removing only the browser-helper directory from the
lookup path (including marked browser-helper directories inherited from another seat); their
arguments, output and exit status are preserved. Separate Cargo shims and
all other PATH entries remain in place. Nothing changes the desktop session, default browser,
MIME configuration, display connection or operator's global environment.

Set `seat_settings.env.MIKE_FLEET_BROWSER_GUARD` to `"0"` to opt out for newly launched seats.
`BROWSER` overrides remain intact and govern helpers that consult that variable; direct-open
containment has its own opt-out. An explicit operator `seat_settings.env.PATH` remains the
base PATH with the browser directory prepended. A full Claude `--settings` override bypasses
fleet defaults entirely. Disabling hooks skips the Bash refresh but keeps prelaunch activation.
This is accidental-launch containment, not an operating-system sandbox: absolute executable
paths, other desktop launchers, shell aliases/functions, deliberate PATH replacement, and pre-existing or external MCP
servers that do not inherit the seat environment are outside its coverage. POSIX-compatible launch shells are required; unsupported Herdr shells fail activation confirmation. Existing seats are
not changed. Return links to the operator instead of opening their desktop from a fleet seat.

**Optional Cargo adapter.** With `execution.cargo: true`, fleet seat settings append a Bash
`PreToolUse` hook that requests native background execution for literal `cargo build`,
`cargo check`, `cargo test` and `cargo nextest run` commands, including plain environment
assignments, toolchain selection, numeric `timeout`, and supported compound shell commands.
It preserves the full command, cwd/environment execution context, timeout and every other
Bash input field; only `run_in_background` changes. It neither executes the command in the
hook nor grants permission. No Cargo executable, PATH or toolchain is replaced, so commands
run by Mike's own queue cannot recurse through this seat-only tool hook.

The local hidden `mike fleet cargo-background-hook` helper reads bounded JSON stdin without
loading configuration, secrets or a database. It requires a `MIKE_RUN_ID` beginning with
`fleet-` and the fleet's `MIKE_FLEET_CARGO_BACKGROUND=1` setting. Existing operator hook arrays
are retained. Set `seat_settings.env.MIKE_FLEET_CARGO_BACKGROUND` to `"0"` to opt out;
`disableAllHooks` and `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1` are respected. An explicit
full `--settings` override can bypass fleet defaults. Managed hook restrictions, an
unsupported client, or another operator input-rewriting hook may also prevent the default
from applying. Unsupported shell syntax, Cargo metadata/run/install, aliases and opaque
wrapper scripts are left alone; use `fleet_check(command)` explicitly for those long jobs.

This safeguard returns Claude's native background task ID and native completion notification,
not a Mike queued-job receipt or Mike completion mail. Native tasks are session-bound and
may stop when that session exits. For durable queueing, deduplication and completion mail,
use `fleet_check(command)`. Neither a native task launch nor a queue acknowledgement is a
passed check. An unconfirmed native completion remains `completion_unknown` in fleet
results, without claiming the process is still running; elapsed time never turns it into a
pass. The protocol follows [Claude's PreToolUse input updates](https://code.claude.com/docs/en/hooks#pretooluse-decision-control)
and [native background task lifecycle](https://code.claude.com/docs/en/interactive-mode#background-bash-commands).

**How a fleet starts and staffs itself.** On `ready` (or `mike fleet start`), `mike serve` opens
the status pane, then the coordinator, the observer and the acceptance seat under session ids it
chose beforehand, so every brief names all three (`launch/fleet/start.rs`, `plan.rs`). The start
report on the fleet row records the status pane's pane and workspace and whether the run created
that workspace; when the fleet reaches done the stop closes the pane and, if the run created it, the
workspace (`status_pane.rs`), so a finished run leaves no pane behind. At the same
time it warms one template for each discipline that has a task
(`launch/fleet/warm.rs`): a session that reads `.claude/fleet/rules.md`, the discipline's load
list under `.claude/fleet/templates/` (the file named for the discipline, else the one named for its
role file) and the fleet's definition, and ends its turn. When the
template's pane reads idle, mike records it as the template `<run id>:<discipline>` and closes
the pane; its transcript is what forks resume. A template that does not load within 480 seconds
is not recorded, and that discipline's workers start cold. Only a template of the same run is
ever forked from.

The status-pane worker then starts a worker for every task that has no seat, is not `done`, and
whose `after` tasks are ready (or accepted/done), without waiting for final acceptance (`launch/fleet/staff.rs`), at fleet start and
again on every 20-second pass, so a task starts within a pass of the verdict that frees it. Before
launching, it writes the seat name, the session id and `state: starting` onto the task with the
definition's optimistic save, so two passes never start the same task; after the launch the state
becomes `working` or `not started: <why>`. There is no limit on how many workers run other than
`launch_max_surfaces`. Clearing a task's `seat` and `session` starts a fresh seat for it.

**Startup baseline.** When `gate.command` is declared, startup captures the current clean fleet-branch SHA and grades it in the managed gate slot before opening seats (`start_baseline.rs`). A dirty tree, changed commit, timeout or unverified result stops startup without discarding edits. A verified failure opens control seats only. The coordinator explicitly nominates a scoped dependency-free task with an existing discipline through `baseline.repair_task`; only that repair task may start until the current clean commit has a matching-command passing gate result. The recorded `baseline` holds the commit, command, result id, pass/verification flags and completion time. Fleets with no declared gate, and existing runs without a baseline record, retain their previous admission behavior.

**mike reads the checkout while a fleet runs.** The status-pane worker starts one watcher per running fleet (`launch/fleet/watch.rs`). Once a second it runs `git status --porcelain=v2` in the run's repository (about 18 ms on a 6,000-file tree) and, when the output moved, reads the index and hashes the changed files; every path that was added, changed or deleted becomes a row in `launch.tree_changes` with its blob hash and the time. Attribution distinguishes direct file-tool evidence from inferred shell candidates. Concurrent shell calls and ownership alone do not prove authorship; ambiguous changes retain candidates and uncertainty rather than naming the owner as author. `GET /api/fleets/{fleet}/changes` (tool `list_fleet_changes`) lists them with the seat, the owning task and a note for a write outside a task's files. `GET /api/fleets/{fleet}/results` (tool `list_fleet_results`) lists the seats' captured Bash commands and observed changes since command start. `still_valid: null` means commit binding, dependency closure or change coverage is unproven; observed invalidation or update mode yields false. `paths` filters displayed changes only. `include_output` adds tool error flags, output tails and exact session/use IDs for full retrieval; tool success does not establish a test pass. Neither asks a seat to write anything down.

**Project-declared operations.** Fleet execution has no language adapter enabled by default.
`execution.cargo: true` opts into Cargo policy, a cargo shim that records build times,
background hooks, Rust language-server defaults and Cargo target-directory validation.
Existing definitions that need those helpers must explicitly add the flag. Project rules
are loaded from that project’s `.claude/fleet/project.md`; generic briefs carry no Cargo
build requirement.

mike holds no machine-wide lock or build queue. A managed command waits only for the
resources its operation names (`execution.operations.<name>.resources`), and builds that
share a target folder take turns through cargo's own lock. How many jobs a build may use is
the project's setting: put it in the definition's `seat_settings.env` (for example
`CARGO_BUILD_JOBS`), not in mike.

A resource entry is either a name, which is exclusive (capacity 1), or an object
`{"name": "mike-rust-build", "capacity": 2}`. An operation takes one unit of each resource
it names and waits when every unit is taken, so a project sets how many of its heavy
operations run at once on its machine. This repository sets `mike-rust-build` to 1 in
`.claude/fleet/operations.json`: one heavy cargo job runs and the next waits for it, because
several at once have exhausted this host's memory. A resource must declare the same capacity in every operation that names it, and a
capacity is between 1 and 64; the definition is refused otherwise. Only declared heavy
operations take a unit; independent operations never wait on each other, and a request
waiting on a full resource holds no unit of the others it names. An operation's brief shows
a counted resource as `name (3 at once)`.

`execution.operations` maps names to exact commands and their contracts. The existing
`fleet_check(command)` / `mike fleet check --command` route selects a declaration by exact
trimmed command equality; wrappers and compound commands must be declared in full. An
operation's `purpose` describes the evidence it produces. A successful exit establishes
command success, not independent acceptance of a task. Undeclared commands remain runnable
and produce receipts, but have no inferred proof purpose or completed-result reuse.

Full fleet and seat views include `operation_adoption`: declaration count, whether the
configured check and gate are declared, and whether the Cargo adapter is enabled. Run
records expose exact declared shell commands as `direct_operation_bypasses`, with an
action to route them through `fleet_check`. Repeat counts preserve the whole command,
including numeric arguments, across TypeScript, Python, documentation and Rust projects.
Wrapped and undeclared commands are outside this measure; zero observed bypasses does
not prove complete adoption.

```json
{
  "execution": {
    "cargo": false,
    "operations": {
      "types": {
        "command": "npm run typecheck",
        "purpose": "frontend type checking",
        "resources": ["my-project-typescript-cache"],
        "environment": {"CI": "true"},
        "inputs": ["src", "package.json", "package-lock.json", "tsconfig.json"],
        "cache_key": "node-and-typescript-versions-from-project-locks-v1"
      },
      "database-tests": {
        "command": "python3 -m unittest tests.test_database",
        "purpose": "database integration assertions",
        "resources": ["my-project-test-database"],
        "timeout_seconds": 600
      },
      "links": {
        "command": "scripts/check-links",
        "purpose": "document links resolve",
        "resources": []
      }
    }
  }
}
```

**Project operation profiles.** Creating a fleet imports the project's checked-in
`.claude/fleet/operations.json` (`schema_version: 1`, `execution` with the schema above).
Mike and gg provide profiles for their actual compile, test, frontend and gate commands.
Explicit definition fields override the profile; each explicit operation replaces its
entire inherited declaration. An operation set to `null` removes it; `execution: null`
opts out of the profile. Invalid profiles fail creation before any seat starts. The
resolved file must remain inside the project. The merged definition is persisted, so
editing the profile does not silently change existing fleets. Keep `check.command`
exactly equal to the declared command to get its resource and reuse contract. Compilation
reuse covers the tracked tree and names its toolchain in `cache_key`; bump that key for
external dependency/environment changes. Tests and browser checks remain uncached.

Resource names are exclusive locks shared by the current user across fleets on this host;
choose distinct names for independent databases/caches and the same name for shared ones.
Locks are acquired in sorted order and retained until the owned process group is cleaned up;
cancellation releases partially acquired locks. Declared operations with independent
resources can execute concurrently in different checkouts. An undeclared command takes the global exclusive lock, excluding declared operations too. Shared checkout access remains
serialized to protect source and build state. Undeclared commands retain conservative host
admission. This routes managed commands only; it does not intercept arbitrary tool calls.
Environment values belong in fleet configuration only when nonsecret; arrange credentials
through the existing process environment. Defaults do not infer dependencies from language.

Run records with `measure_version: run-record-v2-declared-operations` expose
`operation_commands` and `operation_repeats_across_seats`, including per-seat operation
counts. These count exact declared commands observed in captured session tools. Managed
runner executions live separately in `launch.tree_checks`; the record explicitly labels
that coverage. Historical v1 records retain their Cargo fields.

**One configured check shared by every seat.** The definition's `check.command` is the project's own check (here `scripts/cx check -p mike --lib --tests`; elsewhere a build tool's compile), and `check.paths` declares the source paths its answer depends on. Configured checks capture the run branch's SHA and execute in mike's locked, reusable `.claude/worktrees/mike-check` checkout (`check_run.rs`, `check_state.rs`, `gate_isolation.rs`). They do not read the workers' unfinished edits. The managed slot stays detached, retains its own build cache, and must be clean before and after execution at the captured SHA. Since T11 (`d0dca1f6`) there are two such check slots, `mike-check` and `mike-check-2` (`CHECK_SLOT_NAMES`, `gate_slot.rs`): a configured check takes whichever slot's lock is free, so two checks can run at once; the gate still uses its own `mike-gate` slot. Each check appends `check_queued` and `check_finished` ([`FLEET_EVENTS.md`](FLEET_EVENTS.md)) from `check_events.rs`. Mike does not reset, stash or clean worker edits, copy `target/`, or inherit untracked dependency files and checkout-hook setup into the slot. Required ignored dependencies and setup belong in the configured command; managed Git operations disable checkout hooks.

An explicit `commit` (`--commit`), including `HEAD`, resolves the ref before execution and records historical baseline proof in a separate ledger using the same managed check slot (`check_baseline.rs`). `test` (`--test`) uses the declared `check.test_command` template with `{test}` (or appends the quoted test name if the placeholder is absent); it refuses a missing template rather than guessing a cargo command. Focused tests also run in the managed slot. Combine test and commit for a focused historical baseline. These launcher-owned slots do not authorize workers to create additional checkouts.

An explicit `command` without `commit` is different: `fleet_check(command)` queues that exact narrow command in the workers' shared checkout with its existing uncommitted files and warm build directory. Its command-specific receipt does not establish clean-commit proof. A fresh explicit command request does not reuse an old completed result merely because the committed tree is unchanged; a matching active request can join the existing run. Adding `commit` runs the command against the resolved clean baseline instead. All these commands use the managed `sh -c` runner with whole captured output. The default command cap is thirty minutes; a declared operation may set `timeout_seconds` (1–7200). Only the explicit Cargo adapter removes inherited `CARGO_TARGET_DIR`; an operation’s declared environment is then applied.

Completed-result reuse requires an exact command declaration under `execution.operations` with a nonempty `cache_key` and explicit `inputs`. The key includes the operation declaration (including environment and dependency/toolchain key), exact command, source-verification version and each covered committed path's Git mode/type/blob identity (`git ls-tree`). Paths are normalised like task files (`lib/`, `lib/**` and `lib` cover the same folder); `["."]` covers the committed tree (a reusable operation requires explicit inputs), while declared paths matching no committed file are refused. Without a reuse declaration, new requests execute afresh; status still reads the last receipt, and automatic checks avoid repeating an already-recorded commit. A project must change its `cache_key` when any external dependency, toolchain or undeclared ambient environment changes; omit the key when it cannot make that guarantee. A commit changing nothing in that declared scope can reuse the same command's result. This is source-scope evidence, not an inferred dependency closure or a guarantee about ignored dependencies, environment or unrun tests. On each twenty-second pass, `check_run::queue` checks for new fleet-owned commits and can grade the captured newest branch commit when its state has no receipt and no check is in flight; it does not promise a separate execution for every intermediate SHA.

`POST /api/fleets/{fleet}/check` (tool `fleet_check`, command `mike fleet check`) reads or starts the requested check. Compatible completed receipts in `launch.tree_checks` are reused and matching in-flight runs joined; a request for another state while a run is active is explicitly refused as not queued. The default wait is zero: an unfinished answer returns immediately with its receipt, state, commit and start time, and completion is mailed to remembered askers. An explicit `--wait` is capped at 90 seconds. `status: true` (`--status`) reads recorded or in-flight state and starts nothing. Queued, running, recording-result and blocked-completion states are not passes; the result must be durably recorded before completion is announced.

Diagnostics are parsed for `path:line` shapes (rustc's `-->`, Python's `File "…", line`, tsc's `path(line,col)` and plain `path:line`) relative to the execution checkout, then sorted into the asker's files, other tasks' files with their seats, and unowned files. Verified non-baseline configured/focused results notify declared file owners once per receipt (`told`); explicit-command receipts do not trigger that ownership notification. For the ordinary configured check, `check_proof.rs` writes a `result` note tagged `check` for the checked commit's `Fleet-Seat` owner, when identifiable. The note names that SHA and marks it proven only when the passing receipt matches its own covered content state; it does not blanket-prove earlier commits. `askers` records receipt consumers. Seats still use narrow project checks for their own work. The check, commit and gate routes have their own time limits (`api/manifest.rs`); other inspect routes answer within 30 seconds or return 408.

**Where a name is defined.** `GET /api/fleets/{fleet}/where?symbol=…&path=…` (tool `fleet_where`, command `mike fleet where`) runs one `git grep --untracked` over the tree with a definition pattern for the common languages (`fn`, `struct`, `class`, `def`, `function`, `func`, `fun`, `interface`, …) and lists the file, line and text of each definition, plus files named after the symbol (a component, a module). It reads the tree as it is now, so nothing has to be kept current.

The response also includes textual references, up to eight source excerpts, whole-file
paths and SHA-256 hashes, the checkout commit, dirty state and read errors. This is textual
discovery, not a semantic call graph or a claim that all references were found. Complete
files remain available at the returned paths. Ordinary sessions can use
`mike discover --repo . --symbol choose`; `--path RUNBOOK.md` also works for prose, and
`--context-lines 0..40` controls the excerpt radius. Concurrent edits can change a file
between discovery and the next read.

**Bounded discovery pilot.** `scripts/efficiency-pilot prepare --output <new-directory>`
defaults to `--suite realistic`: one multi-file task in each of Rust, TypeScript, Python
and documentation, with callers, configuration, test output and plausible distractors.
`--suite smoke` retains the small fixtures. Preparation randomizes paired arm order and
validates independent graders against broken and corrected fixtures. It
makes no model calls. After quota is available, run
`scripts/efficiency-pilot run --output <same-directory> --model <explicit-model-id>`.
The default is eight calls, at most $1 and 300 seconds each; `--repeats 3` during preparation
creates 24 calls. File tools are the only model tools in this small pilot; trusted graders
execute outside the model's working copy using frozen contracts and only declared editable
files; changed contracts and paths escaping the checkout fail grading. The runner disables client customizations and restricts file tools to the task checkout.
It records provider output, tokens,
tool calls, elapsed time including discovery, configuration and input hashes, activation
evidence and separate grader verdicts. It stops on provider/runner failure. Results are
an initial navigation experiment, not production savings evidence, a test of every memory
delivery path, or permission to widen compression.

**Efficiency evidence.** `scripts/efficiency-report --project /path/to/mike --project
/path/to/gg --output /new/report.json` joins the newest runs (default five per project)
with accepted task obligations, roster-scoped request usage and managed receipts. It
uses bounded read-only database queries and the existing inspect API. Its denominator
requires passed criteria and accepted required obligations, including executable proof
where required. Whole-run elapsed time per accepted task is not individual task latency.
Open runs have no final ratios. Missing usage leaves exact tokens/task unknown; observed
tokens include cache reads/writes and are neither dollar cost nor savings. Spawned sessions
outside the roster are excluded. Historical Cargo-only records do not establish generic
operation counts. Reports expose these coverage limits and keep projects separate.

New completed managed commands write receipt-bound `operation-timing-v1` JSON under
`<state-dir>/managed-builds/timings`. Queue/setup and execution/cleanup durations are
monotonic observations; historical receipts and early failures can lack them. The report
uses `$MIKE_STATE_DIR`, or `$XDG_STATE_HOME/mike` (default `~/.local/state/mike`); set
`MIKE_STATE_DIR` explicitly for a nonstandard installation. The fleet mod records one
best-effort session note when a continuation budget is exhausted. A failed note never
restarts work or retries on every stop; absent historical telemetry is unknown.

`scripts/efficiency-prompt-audit --request <id> --repo /path/to/mike --output /new/audit.json`
compares captured original/sent bodies with local instruction sources. Repeat `--request`
for an explicit sample. Output contains hashes, byte sizes, tool-schema counts and exact
normalized paragraph matches, never captured text. Source overlaps are non-additive;
bytes and words are not measured tokens. A missing match can reflect a different source
revision, and this sampled audit does not establish fleet-wide savings.

**The reference seat.** When a template has loaded, mike forks one more seat from it (`reference`, role `reference`, recorded as `reference.session` on the definition) with a brief that makes it the run's reference: it waits for mail, reads what changed in the area (`list_fleet_changes`), finds the definition (`fleet_where`), reads the file and answers with file and line; it never edits, builds, assigns or decides. Every worker's brief names it. `reference.enabled: false` turns it off; `reference.discipline` picks the template. `outcome-checks.sh` section 8d counts the questions it was asked and the answers it sent, which is the count that says whether it earns its place.

**The run record.** When a run closes (every seat's process exited, or `mike fleet stop`), mike writes one row to `launch.run_records` from the capture tables, once per fleet (a second close keeps the first): per seat the minutes working and waiting (gaps between a seat's main-thread requests, under three minutes and three minutes or more), requests, tool calls, reads of git state, declared operation commands, minutes and calls to its first commit, and the tools offered on its first request against the tools it used; for the run the median and longest minutes to first commit, reports to acceptance and the median and longest minutes until acceptance answered, reads of git state per 100 worker tool calls, declared operation commands and the ones repeated across seats, mail to and from the reference seat, mail by seat pair, and forks that started after a sibling's first request. Every number is measured (`measure_version` names the queries) and none is rewritten by a model. Run record v3 adds the four numbers a run is scored on, all computed from the capture database: time to done, minutes lost to faults (no-seat minutes, relaunch gaps, reports waiting for a verdict, resource waits, checks past their timeout, stopping, and the control seats' model time), tokens per accepted task, and manual interventions (operator turns in seats, hand-started seats, answered operator notes), with the counts that explain them. `mike fleet record <fleet slug|uuid> [--recompute] [--against <fleet>] [--json]` prints them; `--recompute` measures the run again and replaces its stored record, and `--against` prints the difference from another fleet. `GET /api/fleets/{fleet}/record[?against=…][&recompute=true]` returns it, and the `get_fleet_record` MCP tool (full profile; see [`MCP_BRIDGE.md`](MCP_BRIDGE.md)) reads the same route; section 11 of `.claude/fleet/outcome-checks.sh` reads it for the git-read rate and the first-commit figures when it exists and computes them only when it does not.

**Detached retrospective.** Setting `retrospective.enabled` in a fleet definition
replaces its in-run analyst with automatic tripwires and schedules an independent
retrospective after closure; without it, the existing observer remains. The default
is findings-only, described in `.claude/fleet/observer-retro.md`. Its
`MIKE_MCP_PROFILE=retrospective` MCP server exposes six inspection tools plus
creation of ordinary findings notes and ending its own registration. It rejects
unlisted tool calls and binds those writes to the trusted process session, even
when the caller supplies a different `session_id`. Existing-note updates,
carry-over lessons and executable rulings are unavailable. Ordinary MCP profiles
retain their explicit-session and deferred-by-name behavior. Retrospective file
changes require the definition's explicit bounded `write` authorization and a
separate branch.

**Carry-over between runs.** A session note whose `data.carry` is `true` becomes a lesson candidate in the approval queue (`GET /api/improvement/lessons`, source `carry`), scoped to the author's project and to the discipline named in `data.discipline` (else the author's launch agent), with the tag `carry:<slug of the title>`; the lesson text is `data.line` or the title. A later carry note with the same tag and discipline, or a finding tagged `carry:<slug>`, adds one to the item's later-findings count instead of making a second candidate. An approved item is added, one line each, to that discipline's template prompt after the run's part when a fleet is warmed, and its runs-given count moves once per run. Both counts are shown beside the item in the queue; mike marks nothing and decides nothing from them. The lesson worker never consolidates a `carry` candidate: only the operator's approval moves it.

**Reports, verdicts and the state of a task.** A worker reports a slice with `fleet_report`, and acceptance decides with `fleet_verdict`; neither is mail. The report is a `report` note in the seat's notebook carrying the seat's newest commit, the check's verdict at that commit, whether the check ran after the seat's last change (from the change record's `mine` answer), the brief version the seat read and the evidence paths, and acceptance is told once. Since T22 (`d3a5e8ea`) the `mike-mcp` tool is typed: `sha` (or `commit`), `criteria`, `exercised` (`command`, `exit`, `output_ref`), `tests_changed` (paths, or `"none"`) and `unestablished` (`[]` when nothing) are all required, and the bridge refuses a report missing one, naming the field (`crates/mike-mcp/src/tools/fleet_report.rs`). The server refuses the same incomplete report on its own (T13, `ba9bc34e`, `crates/mike/src/api/fleet/report.rs`): "fleet_report is incomplete: `<field>` is missing", for `commit`, `criteria`, `exercised.command`, `exercised.exit`, `exercised.output_ref` or `tests_changed`, and for `tests_changed` that lists `"none"` beside paths.

**The floor.** A task that declares `done_when` (`checks` with an expected exit, `files_changed`, `tests_added_under`, an `exercise` with its command and expected output) has that floor run by mike, as a declared ruling at the reported commit, before any model reads the report (`crates/mike/src/launch/fleet/floor.rs`). The report handler files the report note and spawns the floor; acceptance is mailed the report only when the floor passes. A pass appends `floor_passed`. A failure appends `floor_failed` with the sha and a one-line reason, tells no judge, and mails the reporting seat a code: `check_failed`, `files_not_changed`, `no_test_added`, `exercise_failed` or `floor_unrunnable`. A task with no `done_when` goes to acceptance at once, as before. `floor.rs` is declared in `launch/fleet/mod.rs` (`pub mod floor;`, since T9's `52dbaae2`). Run fields are gone with it: nothing sets a task's state by hand (`set_by_mike` is removed). A verdict is written in the shape the acceptance register already reads, names the brief version it judged, and the seat is told once. A task's shown state is worked out from these notes and the seat (rework, passed, ready, awaiting acceptance while a report waits, working while the seat is open); a hand-set `tasks.<id>.state` is ignored where mike can derive it. The digest lists the reports waiting and for how many minutes.

**The tick.** Each fleet's pass is a list of steps run in order by `run_steps` (`crates/mike/src/launch/fleet/tick.rs`, called from `display.rs`, T9 `52dbaae2`): `staff`, `supervise`, `settle_checks`, `shared_tree`, `judge`, `problems`, `introduce`, `wake`, `tell`, `escalate`, `close` (the list at the time of writing: `STEPS` in `tick.rs` is the authority). A new behaviour is a new entry in `STEPS`, not a new branch in the pass. 

**The fold.** A shown seat, task and fleet state is worked out from the event record, not stored (T9, `af0e83c9`, `crates/mike/src/launch/fleet/fold.rs`; the old `status_cache.rs` is deleted). `fold(events, until)` replays `launch.fleet_events` up to a time in order and yields what was true then: a seat is live once `seat_launch_confirmed` and until `seat_closed` or `seat_process_gone`, and a `seat_launch_failed` seat is never live, so a task whose launches failed has no seat; `seat_of(task)` is the newest live seat of that task. `awaiting_verdict` is a `report_filed` newer than the newest `verdict_recorded`. `exports_passed` is a task's `commit_landed` sha that a `check_finished` passed. A fleet that was recorded `running` or `stopping` is shown `stopping` while a stop was requested and a seat is live, `done` once a stop was requested and none is live, and `done` when seats existed, none is live and every task is accepted. A seat with no recorded `seat_launch_confirmed` gets implied launch and close events from its seat row (`implied_seat_events`): that covers runs recorded before the events existed, and this run's seats until T10's launch events cover every seat. The replay test (`fold_replay_tests.rs`) runs the fold over the three measured runs in `crates/mike/tests/fixtures/fleet-runs/`. The seat behind a `fleet_commit`, a `fleet_check`, a `fleet_gate`, a ruling run and a `fleet_report`, and the task a `what_changed` answer is for, come from the same record (`seated.rs`, #684): a session is a seat while its `seat_launch_confirmed` has no later close, and until the supervisor records that confirmation its open launch row seats it under the task its seat name carries; the definition's `tasks.<id>.session` and `.seat` are not read.

**The supervisor.** A tick step recovers what a person used to (T10, `fd7c7b51`, `crates/mike/src/launch/fleet/supervise.rs`, `supervise_facts.rs`, `supervise_live.rs`). It reads facts, decides actions with pure functions (`decide_seat`, `decide_check`, `decide_slots`) and applies them through an `Effects` trait, so every row of the recovery table is a unit test. A seat with no first request 90 seconds after its launch request appends `seat_launch_failed`, is killed and relaunched up to twice; after that the route is recorded suspect and a needs-you line is raised. A seat whose process is gone appends `seat_process_gone` and is replaced. A seat silent for 5 minutes with unread mail is woken over Remote Control (`seat_woken`); still silent 2 minutes after the wake, it is killed and replaced. A check past its timeout is killed and requeued once, then failed against its task. A dirty or unexpected build slot is recorded suspect, and a needs-you line is raised when every slot is. At a stop each seat is killed up to three times, then recorded `seat_closed` as unkillable with a needs-you line. The `mike-repair` seat and its fallback are removed (`dd2235b9`): the supervisor is the recovery. At run close the fleet appends `seat_closed` for its seats (`d4139316`).

**The shared tree's health.** Every seat works in one checkout, so a broken tree is attributed within a tick (T15, `8275e2bc`, `fdd71654`, `crates/mike/src/launch/fleet/shared_tree.rs`, the `shared_tree` step). The step takes the seats from the event fold, attributes an uncommitted break to the seat whose edit caused it, tells that seat once, tells the reference seat after the tree has been broken ten minutes, and appends `tree_broken_by` when the commit that broke it is known. The status line draws how many minutes the tree has been broken.

**Records, not mail.** A seat's wake and result notices are recorded, not left to mail (T14): `wake.rs` (`c721d235`) wakes a seat that has been silent five minutes with unread mail, once per silence, and a result notice goes to the second seat of a state only; the change record's tail lists the fleet events for the caller's task (`b3c60efb`, `what_changed`).

**The run fields are gone.** The definition no longer stores a task's seat, session or state, and `set_by_mike` is removed (T21, `bbd9b82f`, `df9e6ea7`; staff.rs stopped writing them in T9's `8f31ffee`). `task_run` answers a task's seat, session and state from the derived status, and `definition::task_session` answers a task's live session from the event record. `rg 'tasks\.\w+\.seat|set_by_mike' crates/` returns nothing at the time of writing; T21's sweep may still be changing readers.

**The build wrapper admits.** `mike fleet build-slot` is the shim a seat's `cargo` resolves to. Since T24 (`d1e78611`, `build_slot.rs`) it reads the project's declared cargo resource from `.claude/fleet/operations.json` (the resource the most cargo-running operations claim, at its smallest declared capacity) and takes a unit of it through `build_admission` before a heavy cargo verb (`build`, `check`, `test`, `clippy`, `nextest`, `run`, ...), so seat-side builds queue against the declared capacity as the managed checks do; `build_timing` records `admitted_at` beside `started_at`. Before this, exercise 4 showed all four seat builds running at once. That is the committed behaviour; whether three run and the fourth waits has not been observed on a deployed build.

**The leaves: judge, checker, problem-solver.** Three model calls replace the coordinator, acceptance and observer seats (T17, `da4ff83d`, `crates/mike/src/launch/fleet/leaves/`, prompts in `.claude/fleet/leaves/`). Each takes a typed input and returns an answer that must fit a schema; an answer that does not fit is `LeafError::Invalid`, and a call the budget refuses is `LeafError::Refused`. The judge decides product fit from the diff, the exercise and the criterion text, not from the worker's summary, and is called only after the floor passed (`LeafError::FloorNotPassed` otherwise). Its verdict is `pass`, `fail` or `check`: a `fail` carries `needed`, the one thing the worker must do next, and `verdict_recorded` carries it (`70a0610d`), so the judge's `needed` is on the event record. The floor's failures are `floor_failed` events. The fold does not yet show the two as one open register: T17 removed the reader (`a540e6a3`), and that work is follow-up #681. `check` asks the checker one bounded question per missing fact, once, and the judge must then pass or fail. The problem-solver is handed a dossier (`solver.rs`) for one problem and decides it.

**Problems.** A problem is opened by one of twelve triggers (`problems.rs`: `red_baseline`, `unowned_failure`, `unestablished`, `worker_question`, `disputed_fail`, `criterion_failed_thrice`, `stalled_seat`, `criterion_amended`, `budget_exhausted`, `all_seats_idle`, `environment_fault`, `predating_gate_failure`) and appends `problem_opened`. A decision is one of eleven kinds (`record_fault`, `instruct_seats`, `patch_definition`, `new_task`, `waive_obligation`, `amend_ruling`, `accept_substitute_proof`, `disregard_receipt`, `rerun_targeted`, `change_setting`, `escalate`) with a reason and the evidence read, and appends `problem_decided`. The problem-solver is bounded: 3 decisions per problem, 40 turns and 60 minutes per problem, 20 problems per run. The third decision on one problem appends `problem_escalated`. `POST /api/fleets/{fleet}/decide` (T17 `27c69c4a`) records a decision and reports whether it escalated; the decider runs in the tick: a `problems` step after `shared_tree` (T9 `4c406310`) calls `leaves::pass` (T17 `ff2f8bd5`, `leaves/pass.rs`), which reads the run's events, skips problems past their 60 minutes, hands each active problem's dossier to a `Solver` (`ModelSolver` over an OpenRouter model; any `Solver` in a test) and appends the `problem_decided` events `decisions_for` returns, plus the overdue events for problems past their time. `mike-mcp` exposes a manual decision as the `fleet_decide` tool (`crates/mike-mcp/src/tools/fleet_decide.rs`), which refuses a decision kind outside the eleven.

**A fleet without control seats.** A fleet no longer opens a coordinator, an acceptance seat or an observer (T18, `8a3dab96`): `start.rs`, `plan.rs` and `bundle.rs` stop staffing them, the role files `fleet-coordinator.md` and `fleet-observer.md` and the observer contract are removed, the retrospective is on by default, and the bundle compiles the leaf prompts. The floor, the judge, the checker and the problem-solver do what those seats did. A `fleet_report` goes to the acceptance seat while the run's record seats one (`seated.rs`; the definition's `acceptance.session`, which start still writes for the staffing pass, is not enough on its own). Otherwise the tick's `judge` step (`leaves/judge_pass.rs`) judges it from the record: a task's newest `report_filed` with no `verdict_recorded` for its note, once a `floor_passed` for it is recorded when the task declares a `done_when`, at most two reports a tick and oldest first, so a burst of reports is judged over a few ticks inside the turn judge's share of the LLM governor's rate limit. A call the governor refuses is tried again next tick and nothing is said; any other failure counts, and after five the seat is told once and the report is left until it is filed again. The judge reads the criteria, the typed report, the commit's patch and the task's open obligations, its `verdict_recorded` events are the task's acceptance in `mike fleet status` and the digest (`acceptance.rs`), and the seat is mailed the verdict with what a fail needs. The judge and the problem-solver run on the steerer's model and key (`leaves::model`); without them a report is answered as unjudged and stays awaiting. Every leaf's answer schema is strict, every property listed in `required` and no additional properties, because the codex transport refuses any other shape (`leaves/tests.rs`, `every_leaf_schema_is_strict…`). The tick's last step, `close`, stops a running fleet through `shutdown::stop` once the status says every task is accepted and the run's record seats no coordinator or acceptance seat; a fleet that has one is closed by it.

**Questions between seats.** `ask` writes a row in `workspace.questions` for the recipient (a session, `task:<id>`, `owner:<path>` or the reference seat), with the asker, whether the asker is blocked and the note it is about. It stays in the recipient's `what_changed` view `standing` until `answer` or `decline` settles it, and the asker is mailed once. On the fleet tick (`launch/fleet/questions.rs`) a blocked question open five minutes is mailed once to the reference seat and listed in the digest. An answer given with `amend: true` adds the question and the answer to the end of the brief part the question is about, as a new note version, so the next seat reads it.

**Tasks added during a run.** `fleet_propose` adds a task to a running fleet when every file exists or is listed as new, every criterion, `after` task and the discipline exist and no open task owns a file; the new task gets the next id and the staffing pass starts its seat. A file an open task owns holds the proposal (the definition's `proposals` and the task's `held` entry record it) and that task's seat is mailed: it releases the file by editing its task, or declines. `fleet_split` moves named files and criteria from the caller's task into a new task after it. Two definition items extend this: `staffing.adopt_unowned` (on unless the definition sets it off, since T16, `0d95e0c2`) has mike propose a task once for a change in the checkout that no task owns after ten minutes and for a check failing in files no task owns, and tells the coordinator; `staffing.warm_siblings` (off unless set) starts one fork of a template first and the others after its first request has returned, so the later forks read the prefix the first wrote. A fork that waited records `sibling_wait_seconds` and `waited_for_seat` on its `launch.launches` row.

**A linked worktree, and a push that does not work.** `define_fleet` accepts a linked git worktree as the fleet's `repo`; its seats are launched there, the commit hooks come from the main checkout's git directory, and the answer's `at_ready.notes` says once that the worktree lacks the main checkout's untracked project files. At ready mike runs `git push --dry-run origin <branch>` from the checkout; when that fails for want of credentials and `gh` is logged in, it sets the checkout's `credential.https://github.com.helper` to `!gh auth git-credential` and tries once more, and a fleet that still cannot push does not go ready (`at_ready.machine` gives git's words). A commit whose push fails carries `push_error` with git's words in every `fleet_commit` answer, and the coordinator is mailed once per run (`[mike push]`). The once-only record of mike's own mail is the table `launch.told`, so a restart does not send a notice again; an introduction row is written only when its mail is sent. Each seat is launched with its kind's tool profile (see [`MCP_BRIDGE.md`](MCP_BRIDGE.md)).

**Repeated commands carry receipts, not a requirement to contact a peer.** The introducer matches normalized working commands across different seats, excluding shell utilities, git reads, inline scripts and heredoc contents (`launch/fleet/work_of.rs`). A proactive notice requires an authoritative measured duration of at least five seconds for each matching command in both invocations. A compound shell invocation's total time cannot establish the duration of one of its subcommands. Background launch receipts and response-to-result capture latency do not establish completed, slow commands. Legacy Bash captures contain no authoritative command-duration receipt, so they currently produce no proactive introductions. This is an evidence limit, not evidence that repeated work was eliminated. Recorded results remain available through `list_fleet_results` and `list_session_tool_uses`.

The configured fleet check's path list is not the dependency closure of an arbitrary cargo build, run or nextest invocation. The conservative fallback considers every observed checkout change from the first command's start through the latest observation; any such change invalidates the introduction. An empty change log does not establish complete event coverage or a reusable result. A notice carries a bounded, explicitly labelled command preview, exact session/use IDs, recorded tool outcome, measured durations, unknown commit binding, false-or-unknown `still_valid`, and a precise full-command/full-output retrieval reference. It never turns tool success into a test or lint pass. This receipt-first handoff applies to open, closed and accepted seats alike; the coordinator does not need to mail an unavailable first runner. Claims remain once-only and are released when mail is not queued. The separate reuse decision also considers all observed checkout changes since command start and returns `unknown`, with the captured result reference, when no change was observed but dependency validity is unproven.

**What the operator must see.** The status (`GET /api/fleets/{fleet}/status`, `mike fleet status`) carries `needs_operator`, every note of the run with `status: "operator"`, and `every_seat_idle_seconds`, set when the fleet is running, no open seat is working or starting, and the newest request of any open seat is five minutes old or more. The pane draws them straight under the header as NEEDS YOU and NOTHING IS MOVING, and `mike fleet summary` adds them to its one line. On 2026-10-01 a fleet stood for 45 minutes behind a coordinator's note the pane did not show.

**A goal per worker.** The definition's `goals.workers` and `tasks.<id>.goal` are completion conditions in the words of Claude Code's `/goal`, empty unless set. `launch/fleet/goals.rs` resolves one per seat: control seats (coordinator, acceptance, observer) never have one, because digests and mail already wake them and a goal that cannot progress until workers report re-woke them every few seconds with nothing to do (mike#545); `goals.fleet`, `goals.coordinator`, `goals.acceptance` and `goals.observer` in an older definition are ignored. A worker reads its task’s `goal` or else `workers`; `{criteria}` expands to criterion ids and that task’s assigned contribution, never the full fleet outcome. A bounding suffix limits evidence to its own `Fleet-Seat` SHA(s), exercise and report, independent of shared HEAD or acceptance. Once all actionable work is exhausted, reporting an external blocker and next action ends the loop for now without accepting the task. Templates and the reference seat never have a goal. The status (`GET /api/fleets/{fleet}/status`, and `by-session`) carries each seat's `session` and resolved `goal`. The fleet-comms mod, loaded in every seat, finds its own row by session after each turn and runs the built-in `/goal` with the text when it differs from the one it armed; Claude Code judges the condition after each turn. Independently, the mod counts main-session Stop-hook re-entries in its persistent store and stops automatic continuation after 20. Set `seat_settings.env.MIKE_FLEET_GOAL_MAX_CONTINUATIONS` to an integer from 1 to 200 to change that limit. Exhaustion clears the built-in goal without accepting the task or deleting its fleet goal or transcript. Restarting or receiving mail does not replenish the budget; mail can still start a normal turn. A changed fleet goal or a new non-meta request typed at the terminal renews it. Remote Control and SDK prompts do not replenish it: they can carry automated work, including mike mail over Remote Control. Child-agent stops do not count. mike's own check lists a goal over 4,000 characters once filled in, or a worker goal with an unknown placeholder, as missing. Measured 2026-10-08 on Claude Code 2.1.294: a mod can run `/goal` from `turn.complete` but not from a `command.run` hook, and a first prompt that starts with `/goal` arms it too.

**The gate grades a captured commit in a managed checkout.** The definition's `gate.command` is the project's quick, non-mutating per-push gate; acceptance arranges the full gate once at close. `POST /api/fleets/{fleet}/gate` (command `mike fleet gate`, for the acceptance seat; no tool) captures the local fleet branch's tip and runs in the locked, reusable `.claude/worktrees/mike-gate` slot (`gate.rs`, `gate_execution.rs`, `gate_isolation.rs`). The automatic queue targets `origin/<branch>` when available, falling back to the local branch, and reports whether the target is pushed. Workers' uncommitted source is excluded. The slot keeps its own cache; dependency files such as `.env` and `node_modules` are not copied from the workers' checkout. The command must prepare required ignored dependencies itself. A dirty or unexpected slot is refused without resetting, cleaning or stashing it, and the result cannot prove the commit if its source is unverified after execution. Legacy `gate.link` and `gate.warm` fields are ignored.

Gate receipts live in `launch.tree_checks` under `<run>#gate:isolated-source-v2` with an execution-policy hash suffix when an operation or Cargo adapter is declared, and the captured commit as state. Changing the declared environment, dependency key or adapter invalidates the receipt. A new request reuses completed results only for a declared operation with `cache_key`; status and the automatic queue can still read an already-recorded verdict. Reuse also requires the exact gate command; old shared-source receipts do not qualify. Acceptance can request a run and any seat can read `--status`, which starts nothing. Status and completion identify the actual in-flight or recorded command, not a subsequently edited definition. The default request wait is 45 seconds; if the run continues, the caller receives its running state and later completion mail. The command has a two-hour cap, reduced by a declared operation’s `timeout_seconds` when present. Acceptance also receives completion with the full SHA, command, recognized failure inventory and an explicit warning when source verification failed. Missing or unparsed tests are not implied to have run.

Full output is retained in the persisted result and an execution-specific log beneath the gate slot's `.claude/worktrees/`, named with the captured SHA, command hash and execution id (`gate_queue::completion_log`). A latest-output log is also kept there. The returned `output_file` references the execution-specific log, or the persisted result id when that log is unavailable; a later run cannot silently replace the earlier run's evidence through the latest-output file. Managed directories are excluded from Git status.

**Late gate failures retain task context.** Named failures are matched against files committed at the gate's SHA, then routed through current declared ownership (`gate_failure_routes.rs`, `gate_failure_delivery.rs`). This identifies contacts, not who introduced a regression. Closed owners fall back to an open successor or coordinator. Unknown, ambiguous or undeliverable matches are sent to the coordinator; unsuccessful delivery is logged while the full gate result remains retained. Bounded notices include up to three newest retained report references per matched task, each with its original session, reported commit and notebook lookup (`gate_failure_handoff.rs`). Older omitted reports, absent reports, unusable SHAs and unavailable note reads are stated explicitly. Summaries, scenarios, commands, proof and evidence remain retrievable from the original notes where provided, including after the author's seat closes; they are not asserted to prove the gate's commit. Workers still end after bounded task handoff rather than waiting for a global gate. Declaring cross-cutting test files in `criteria.paths` and assigning them through tasks' `files` or `new_files` checks ownership coverage, not early execution or detection latency.

**What mike tells a seat once.** On the twenty-second pass mike also reads the seats' commands of the last forty minutes (`launch/fleet/tell.rs`) and mails, once per notice key recorded in the persisted run ledger (`told.rs`): a seat that ran the check's own program itself (the same leading words as `check.command`, up to the first flag), to ask `fleet_check`; and a seat that ran `git worktree add`, with a copy to the coordinator. The reference seat is mailed (`tell_reference.rs`) each acceptance FAIL of the last ten minutes with the seat that did the work, and one list of the seats ten minutes into a task with no commit (read from the `Fleet-Seat` trailers of the branch since the run started). Every seat is launched with `CLAUDE_BASH_MAINTAIN_PROJECT_WORKING_DIR=1`, so its shell returns to the checkout after each command: Claude Code makes an agent's memory folder under the shell's current folder at each turn, which left empty `.claude/agent-memory/<agent>` folders wherever a seat had changed directory.

**Commits go through mike.** `POST /api/fleets/{fleet}/commit` (tool `fleet_commit`, command `mike fleet commit -m ...` in a launched pane, which knows its session from `MIKE_SESSION_ID` and its run from `MIKE_RUN_ID`) commits a seat's work without the shared index (`launch/fleet/commit.rs`): it takes the changed files the seat’s task owns, excluding foreign-owned files even when the change record attributes them to that seat; an explicitly named file still permits an ownership override with a warning, while a named folder skips foreign-owned files and never refuses on a judgement (the operator's ruling of 2026-10-01: refusing is not its job): a path outside the task's files, a check that fails in the files committed, the definition's `commit_check` failing on them (`launch/fleet/commit_check.rs`: the declared command run with the committed files of the declared endings as arguments), and a committed `.rs` file that adds `mod <name>;` while neither `<name>.rs` nor `<name>/mod.rs` (resolved the Rust way from the declaring file) is in the commit's tree (`launch/fleet/commit_modules.rs`; the warning names the files and the task that lists them) come back as `warnings` on the report. After a commit lands mike writes a `result` note for the seat (`commit_note.rs`): the message's body, each file with its added and deleted lines, how it was verified and the warnings. It says how the commit was verified (`launch/fleet/commit_verify.rs`: the tree of the commit is built first, and the commit is proven when a declared reusable operation has already passed at that tree’s state and execution-policy identity, which is the commit’s own content whatever other seats leave uncommitted; source-only legacy change events cannot establish that policy; otherwise the `Fleet-Verified` trailer says it is not yet proven and that mike checks the commit once it lands, and the `result` note of that run records the verdict against the commit's sha), builds the tree from the current head plus only those paths in a private index under one lock per repository, runs `.git/hooks/pre-commit` against that index (and only that file: a hook kept elsewhere through `core.hooksPath` is not run, which the operator confirmed on 2026-10-01, because in a shared tree such a hook scans every seat's unfinished files and the acceptance seat's gate runs the same scans), writes the commit with a `Fleet-Seat` trailer, moves the branch only if the head is still the one it read, refreshes the real index for those paths so they read clean, and pushes, merging first if the push is rejected. The server process often has no credentials for the remote (a token that lives only in session shells), so a failed push comes back as `push_error` with git's own words, and the caller (the MCP bridge in the seat's session, or `mike fleet commit` in its pane) then pushes from its own environment. When that push is refused because the remote holds commits the checkout does not (a push from outside the fleet), the caller fetches, asks `POST /api/fleets/{fleet}/integrate`, which merges `origin/<branch>` into the checkout under the commit lock and aborts the merge if it would touch a file a seat has not committed, and pushes again (`launch/fleet/push.rs`). Every launched process also carries `MIKE_SESSION_ID`, `MIKE_RUN_ID` and `MIKE_SEAT` in its environment.

A fleet also ends without being stopped. The status-pane worker in `mike serve` checks every
running fleet every 20 seconds: an open seat whose recorded process id is gone is recorded closed
with reason `process exited`, and once no open seat is left (closing the fleet's workspace does
this) the run is closed, its templates are retired, the fleet is marked `done` and the terminal backend shows a notice. A seat
with no recorded process id counts as live, so it never ends a fleet on its own.

`--watch N` clears the screen and redraws every N seconds until interrupted; N must be at least 1.
A failed call is drawn where the frame would be and the loop keeps going, so a proxy restart does
not end a watch.

The drawing is `launch::fleet::render::render(&FleetStatus, width)`, a pure function of the status
the API answered. The width is the terminal's, then `COLUMNS`, then 170. A row wider than the width
is cut and ends with `…`; no row is wrapped. Colours are 24-bit ANSI codes and are dropped when
standard output is not a terminal or `NO_COLOR` is set. A seat's colour comes from its role, from
one table in `render/ink.rs`. The context bar is 20 cells for a 1,000,000-token window with a mark
at 600,000, where a seat is replaced: green below 400,000, yellow below 600,000, red from there,
with the word `replace` after the number.

Exit codes: `0` when the call succeeded, `1` when the API answered with an error (a `409` from
`start` prints its message) or could not be reached; an unreachable API is reported with the
address that was tried.

### Reference fleets

A reference fleet is a definition, a repository state and a known answer, run through the
inspect API and scored by the run record. The smoke reference, `fleet-ref-node`, is a small
TypeScript package under `crates/mike/tests/fixtures/fleet-ref-node/` with five tasks
defined in `scripts/fleet-refs/node.json`: two independent, one on the first's scaffold,
one that fails its floor once and one whose brief names a missing file. The known answers
sit under the fixture's `.answers/` and are put on a branch of a separate repository the
fleet cannot see.

- `scripts/fleet-ref prepare [node] [--variant red-baseline]` copies the fixture to a
  temporary directory with a bare origin beside it. The `red-baseline` variant has no
  `node_modules`, so the baseline is red for a recipe fault.
- `scripts/fleet-ref run node --drill` defines the fleet and never starts it, and prints
  what the definition is missing and how many seats it would start. Use it while a real
  fleet runs: a second running fleet would exceed the pane limit. Set
  `FLEET_REF_NO_INSTALL=1` to skip the `npm install` the script otherwise runs in the
  prepared copy.
- `scripts/fleet-ref run node` defines, readies, starts and waits for done, then prints
  the record; it is run once by acceptance at the close, with worker seats closed.
- `scripts/fleet-ref record <slug>` prints the four numbers of a run.

The Rust reference, `fleet-ref-rust` (`scripts/fleet-refs/rust.json`), is mike itself at a
pinned commit in a long-lived worktree slot, `.claude/worktrees/fleet-ref`, which keeps its
`target/` between runs: `prepare` checks the slot out on a new branch at the pin and
reverts six small changes (the known answers, each a commit) in one commit, and builds
nothing. Four of the six are in different crates and areas and two are in one, so both the
shared tree and the commit-before-branch paths are exercised. It is defined but not yet
built or run: its first run, one cold build, follows the deploy.

### Not a deploy gate

A deploy never runs a reference fleet. The deploy gate that ran the smoke reference on the
live binary before the cutover and on the new one after it was removed on 2026-10-26: it
made every deploy that touched the fleet path wait on a whole fleet run, and with the proxy
down it could not finish at all. Run `scripts/fleet-ref run node` by hand when a change to
the fleet path needs it.

### Eval runs of kind `fleet` (T5)

A reference run is scored as an eval run of kind `fleet` (`crates/mike/src/eval/fleet/`,
`crates/mike/src/config/subcommands/eval/fleet_kind.rs`). Its items are run records and its
metrics are the four numbers of the run record: `time_to_done_minutes`, `fault_minutes`,
`tokens_per_accepted_task` and `manual_interventions`. A number may be a plain value or an
object with a `total`.

- `mike eval run --kind fleet --record-file <file>` (or `--fleet-run <fleet uuid>`, which
  reads the stored record) prints the four numbers as JSON. Exit `0` when all four are
  present, `2` when the record lacks one.
- `mike eval gate --kind fleet --baseline-record <file> --candidate-record <file>
  [--history-record <file>…] [--json]` compares a before-record with an after-record. A
  metric worsened when it rose by more than its allowance. The allowance is 10% of the
  before value; with ten or more `--history-record` runs it is two standard deviations of
  that metric across them. Exit `0` (pass) when no metric worsened, `1` (fail) when any
  did, `2` when either record lacks a number. The report lists each metric's before, after,
  allowance and whether it worsened. This is the gate the deploy runs.
- A reference run's judge verdicts are labelled from the known answer: a task whose answer
  was re-landed with its test passing is `accepted`, otherwise `corrected`, and the judge's
  accuracy is the share of its verdicts that agree.

### The plan: `done_when`, `exports` and the compile-unit warning (T6)

A task in a fleet definition may carry two more fields, both optional and checked when the
definition is checked for ready (`crates/mike/src/launch/fleet/checks_paths.rs`).

- `exports`: repository-relative paths the task writes for others to use. Each must be a
  plain path (no `..`, no glob) and one the task owns in `files` or `new_files`.
- `done_when`: what shows the task is finished, with at least one of `checks` (each an
  `operation` declared under `execution.operations` and an `expected_exit` from 0 to 125),
  `files_changed` and `tests_added_under` (paths the task owns), and `exercise` (a
  `command` and the `expect_output` that shows it worked, both non-empty). A `done_when`
  naming an undeclared operation, an unowned path or an empty body is refused.
- A warning, not a refusal: when two tasks that are not done have files in the same
  compile unit (one Cargo package, or one `package.json` workspace member), the ready check
  names the unit and the tasks, because one broken edit stops the build for all of them.
  Give the unit to one task, or order the tasks with `after`.

The run fields stay in this fleet's definition; a later fleet removes them.

## 6. See also

- [`MCP_BRIDGE.md`](MCP_BRIDGE.md) — the `spawn_session`/`list_launches`/`kill_session` MCP tools.
- [`SCHEMA.md`](SCHEMA.md#domain-launch) — the `launch` schema.
- [`CONFIGURATION.md`](CONFIGURATION.md) — every `launch_*` / `MIKE_LAUNCH_*` config knob.
- [`docs/tui/screens/setup.md`](tui/screens/setup.md) — the setup wizard's terminal-backend step.
- The fleet protocol (`.claude/fleet/`; `~/.claude/commands/fleet.md` is a link to
  `.claude/fleet/fleet-command.md`) is this domain's primary consumer. `.claude/fleet/rules.md`
  is how every seat works; the role files under `.claude/agents/fleet-*.md` say what each control
  seat does.
- [`animations/fleet-run.html`](animations/fleet-run.html) — an interactive walkthrough of a fleet
  run built on these verbs; open it in a browser from a checkout. A recording of its scripted
  run:

  ![Recording of the fleet-run demo: the coordinator warms templates, forks worker seats, and sends them work through the mailbox; acceptance sends rework back to a worker; the observer reports a fenced-file finding; seats are killed when the run ends](animations/fleet-run.webp)

### Cross-task acceptance regressions

`fleet_verdict` accepts an optional `regressions` array from the fleet's current
acceptance seat. Each entry names another `task`, nonempty `criteria` belonging
to it, a `reason`, `failing_tests`, and `evidence`. Attribution is explicit;
words in a verdict or a reference-seat message never assign ownership.

Mike stores each regression as a required, failed `acceptance-obligation` before
writing the original task's verdict, and attempts to notify the target's live
seat or successor. A failed obligation write prevents the original verdict.
The response's `regression_obligations` contains the saved note UUIDs. If a
later write fails, already-saved obligations remain blocking and visible.
Acceptance resolves them through the existing obligation register workflow;
a pass for the originating task does not resolve the target's regression.

Generic session-note creation and updates carrying `acceptance-obligation`
validate the effective note against the tagged fleet's locked definition in the
same transaction as the write. Invalid author, kind, outcome, observation, or
task/criterion scope is refused without changing the stored note. Existing
invalid rows remain visible to readers and cannot reopen unrelated tasks.

### Executable acceptance rulings

Acceptance can register a versioned command and expected exit code on a scoped
obligation. `fleet_check(ruling=<note UUID>, commit=<SHA>)` runs it in the clean
historical checkout; `fleet_report` and fleet acceptance read its exact durable
receipt. See `FLEET_RULINGS.md` for registration, immutable
versions, missing-proof states, and successor behavior.
