---
title: Features
status: active
created: 2026-07-08
updated: 2026-11-06
summary: What the web UI shows, and the complete table of optional features — what each does, whether it rewrites traffic or sends captured text off the machine, its default, and where to turn it on.
---

# Features

This is the day-2 guide: the stack is up (see [`GETTING_STARTED.md`](GETTING_STARTED.md)), Claude
Code is pointed at `http://localhost:9988`, and you want to know what the web UI shows you and
what mike can do to your traffic. For design rationale see [`ARCHITECTURE.md`](ARCHITECTURE.md);
for the full request lifecycle see [`REQUEST_FLOW.md`](REQUEST_FLOW.md).

**A fresh install captures, forwards, analyses and maintains local state.** Defaults vary by
feature; use the Default column and the [delivery exceptions](#things-that-are-not-feature-toggles)
below rather than assuming every feature is off. An install that
already has settings stored keeps them — a stored value always wins over the code default, and
a default change never rewrites stored values, so upgrading does not silently change what mike
does to your traffic.

## Using the web UI

Open <http://localhost:9990/> once the stack is up.

The UI is organised around **sessions** — Claude Code runs in sessions, and the UI groups
requests the same way:

- **Sidebar** — projects, with sessions nested underneath. Click a session to load it.
- **Session timeline** (`/sessions/[sessionId]`) — the conversation as a sequence of
  human-prompt / assistant-response pairs, in time order, newest turn anchored to the bottom.
- **Session context panel** — the system prompt that was in effect for the session.
  Collapsible, persisted locally.
- **Request detail** (`/sessions/[sessionId]/requests/[requestId]`) — headers, params, request
  body, response body, diagnostics, and the session-level system prompt, as separate
  collapsible cards.
- **Analysis dashboard** (`/analysis`) — aggregates analyser hits by category and pattern. With
  persistence on (the default) this queries Postgres directly; it falls back to the in-memory
  store's own diagnostic stats only when persistence is disabled.

Other surfaces: `/projects`, `/prompts`, `/tools`, `/agents`, `/memory`, `/comms`, `/fleet`,
`/steering`, `/filter/workbench`, `/diagnostics/memory`, `/diagnostics/agent-dispatches`, and
`/help` with its guides. Settings live at `/settings` with sub-pages for `assistants`,
`embedding`, `flagging`, `history`, `integrations`, `setup`, `steering` and `workers`.

The page updates live via WebSocket as new requests come through.

## Response analysis and optional diagnostic reminders

Every response, streaming or not, runs through a closed set of 19 regex patterns across five
categories (`JustifyingShortcuts`, `DeflectingBlame`, `AvoidingWork`, `FakingUnderstanding`,
`Overselling`) at `ResponseComplete` time. Hits are attached to the request as diagnostics and
shown in the request detail view and the `/analysis` dashboard. The analyser itself only
records findings. If the `reminders` toggle is enabled,
`RemindersStage` can turn available prior-request findings into a `<system-reminder>` on a
later primary request, or deliver it over Remote Control.

This is governed by its own `analysis.*` settings rather than a feature toggle. It is purely
local — no text leaves the machine for it.

## Optional features

Rows in the first three tables below are off unless the Default column says otherwise; the
tables after them list the switches that ship on. "Rewrites traffic" means the bytes
mike forwards to Anthropic, or the bytes the client receives, differ from what the other side
sent. "Sends text out" means captured conversation text is transmitted to a service other than
`upstream_url`.

Turn any of these on at <http://localhost:9990/settings>. The feature toggles are also readable
and writable over REST (`GET /api/settings/features`, `PATCH /api/settings/features/{key}`);
the finer-grained settings use `PATCH /api/settings/{key}`.

### Three starting points: `mike features`

`mike features watch-only | recommended | everything` sets every switch at once through the
running proxy, reads them back and reports any that differ. It is the command behind the setup
wizard's features step, and the wizard starts on **recommended**.

| Choice | What it sets |
|---|---|
| `watch-only` | Every switch at its default. The switches that rewrite requests or send captured text to another service are off. |
| `recommended` | Watch only, plus `recall_injector`, `agent_lessons` and `reminders`, and the `reminders.inject_lessons` setting (lessons from earlier sessions on the first turn). All four add a `<system-reminder>` to what Claude Code sends to Anthropic; none sends text anywhere else. |
| `everything` | Every switch in the table of switches, including the ones that call a model and the ones that rewrite requests, plus `reminders.inject_lessons`. The ones that call a model do nothing until a provider is set up (`mike setup models`). |

The proxy keeps switch settings in its database, so the command needs persistence on. When the
proxy is not running, the choice is saved as `setup_features` in the per-user config file and the
proxy applies it once, then removes the key, at its next start.

### Rewrites what mike forwards to Anthropic

| Feature | Toggle / setting | Default | What it does |
|---|---|---|---|
| Diagnostic reminders | `reminders` | off | Appends a `<system-reminder>` built from the diagnostic findings on the previous request. |
| Recall injection | `recall_injector` | off | Appends a `<system-reminder>` of topic-matched lessons read from Postgres. Reads the database only; no network call unless embeddings are also on. |
| Session lessons | `reminders_inject_lessons` (setting) | off | Appends a `<system-reminder>` of session-start lessons on the first primary turn. |
| Agent lessons | `agent_lessons` | off | Appends persona-scoped lessons on subagent turns. |
| Self-identity reminder | `self_identity_reminder` | off | Appends a self-identity `<system-reminder>` on primary turns. |
| Mailbox redelivery | `mailbox_redelivery` | off | Re-injects cross-session mailbox messages that were delivered but not consumed. |
| Steerer injection | `steerer` | off | Runs the steerer fleet, which watches sessions for goal and anchor drift and queues `<system-reminder>` blocks for the next turn. **Also sends text to OpenRouter** — see below. |
| Proxy-declared tools | `proxy_tools` | off | Gives a session mike's own tools (currently `mike_session_roster`) when it does not already have them, by appending the definitions to the request's `tools[]` and answering the calls inside the proxy — see the response section. Three kinds of request are left alone: one from a session that brought the tools itself (its requests name the MCP tool `mcp__<server>__get_workspace_roster`, loaded or listed as loadable; once seen, the session is remembered); a sidecar call that shares nothing with the session (session title, web-page summary, monitor, quota probe, advisor); and a request whose own `tools[]` is empty. The next-prompt-suggestion and count-tokens calls re-send the session's tool list to reuse its prompt cache, so they follow the session. |
| Repeated notices | `repeated_notices` | off | Claude Code adds a notice to every message another session pushes to a session ("This came from another Claude session …") and to every mike channel message that arrives mid-turn ("IMPORTANT: This is NOT from your user …"), and it closes every message the operator sends while the session is working with one sentence ("This is how Claude Code surfaces messages the user sends mid-turn …"). With this on, a session is sent each of the three once: the first copy stays and later copies are removed from the **forwarded** body, wherever Claude Code put them (system text, user text, or the reminder blocks after a tool's output). The captured copy keeps the original bytes. A session picks the switch up when it starts; a session that is already running picks it up when the proxy next restarts. |
| Read deduplication | `compression_dedup.enabled` (setting) | off | Replaces duplicate Read/`cat` tool-result content in the **forwarded** body. The captured copy keeps the original bytes, so what mike stores and what Anthropic received differ. |
| Prompt compositions | `composition_enabled` (boot config) | off | With a composition in `template` mode, splices the composition's blocks into `system[]`. `observe` mode records the prompt identity without changing bytes; `replace` mode is not implemented and falls back to passthrough. |
| Reminder-on-token | `reminder_on_token` | off | **Read the caveat below** — this stage changes the body whether the toggle is on or off, when a hook sentinel token is present. |
| File-read compression | `file_read_compression` | off | Shortens large file reads in a conversation's history: an older read is replaced by a pointer when a newer whole read of the same file follows, and other reads keep their head and tail once the model has answered `file_read_compression.grace_turns` times (default 3) since the read, or sooner when the file was edited since. The newest result, protected paths and all explicitly ranged reads stay whole, including after compaction or when reduction markers exist only in forwarded history. Rereading a path returns its current version. Policy v4 markers also carry `get_request_body` arguments (`id`, `tool_use_id`, `sha256`) for the exact captured original result. Recovery rejects a missing, ambiguous or mismatched result; it does not depend on the current file. `file_read_compression.mode` is `shadow` by default, with `file_read_compression.forward_share` at 0: each request's manifest records what would be shortened and the request is sent unchanged. In `shadow`, the percent of new sessions set by `forward_share` are sent shortened requests and the rest are the control; `forward` sends every new session shortened requests. Judge it with `scripts/compressor-trial-report` on paired runs, not on bytes. |
| Shell-output compression | `shell_output_compression` | off | Shortens older Bash output in a conversation's history to its first `head_lines` and last `tail_lines` (default 20 each) plus every error, failure, warning or summary line with `signal_context_lines` after it (default 3, at most `signal_line_cap` = 40). Each omitted range is marked `[[mike:shell_output_reduced]]`, and the result ends with `get_request_body` arguments (`id`, `tool_use_id`, `sha256`) for the exact captured output. The newest result, error results, output under `min_bytes` (1,500), output a hook, mod, tether or mike added (`<system-reminder`, `[[mike:`), and output the model has answered fewer than `grace_turns` (3) times since stay whole. A cut must save 25%. On the forward arm a new cut waits until the conversation's prompt cache is cold. `shell_output_compression.mode` (default `shadow`) and `shell_output_compression.forward_share` (default 0) work as for file reads, with arms assigned independently. `scripts/compressor-loop` scores the arms per turn and per lane and counts cuts whose hidden lines were used later. |

**Compression widening evidence.** The compressor scorecard isolates follow-ups by session and agent, deduplicates tool ids,
uses a thirty-minute observation window, and reports missing identity/result coverage and
right-censored exposures. Sessions with mixed arms or incomplete usage are excluded from
cost comparisons; arm observations are read in bounded one-day chunks. A later read is an association, not proof of recovery. Widening
requires `compressor-qualification-v1` evidence matching the active policy, completed within
seven days: passing original/compact executions for at least three distinct tasks in each
of documentation, configuration and test-output, complete usage/results, activation and
constraint verdict references, and a recovery test whose weakened control fails. Missing
evidence blocks widening; a simultaneous protective policy change also blocks it. Existing
settings are preserved. Whole-file rereads without a range or retained recovery marker do
not carry the explicit-range guarantee.

### Changes what the client receives

| Feature | Toggle / setting | Default | What it does |
|---|---|---|---|
| Weasel intercept | `weasel_intercept` (plus a non-empty `weasel_intercept_forbidden_words`) | off, and the word list is empty | Watches a streamed response as it passes, holding back only the newest few characters of text. If a forbidden word completes, the reply is cut before it, mike re-asks `upstream_url` with a correction prompt, and the corrected answer continues the same message. No part of the word reaches the client; the text before it already has. A buffered (non-streamed) response is not scanned, and that is recorded. |
| Proxy-declared tools | `proxy_tools` | off | Watches the response of a request it added a tool to, and of no other request. The reply streams as it arrives. If the model calls a proxy-declared tool, that call is held back, mike answers it in-process from its own database, splices the tool result into a follow-up request, and continues the same message with that answer. |

### Sends captured text off the machine

All of these are inert without an OpenRouter API key
(`MIKE_STEERER_OPENROUTER_API_KEY`, or `OPENROUTER_API_KEY`), except the embedding row.

| Feature | Toggle / setting | Default | Needs OpenRouter key | What is sent, and where |
|---|---|---|---|---|
| Steerer fleet | `steerer` | off | yes | Prompts, assistant text, tool-call summaries and trajectory digests, to `openrouter.ai`. |
| Turn judge | `judge_worker` | off | yes | Each turn's user prompt plus the final assistant text. |
| Session judge | `session_judge` | off | yes | A multi-turn digest: each turn's prompt and final response. |
| Turn guard | `turn_guard` | off | yes | At each turn end and after quiescence, the turn's closing text, its dossier facts and (with `guard.calibration.input_mode = live`) a calibration block, to four guard questions. Every question starts in `shadow`: rows are recorded, nothing is delivered. A question the operator promotes to `append` or `wake` delivers a nudge, which changes traffic. |
| Semantic context | `semantic_context` | on, loopback only | no | Operator-turn prompts, endings and anchors, to the registered embedder. Each call is refused unless the embedder URL is loopback, so nothing leaves the machine without an operator changing the embedder. |
| Verifier input budget | `steerer.verifier.input_budget` (setting) | `verifier-input-v1` | yes | Narrows what the already-enabled verifier sends: each section of its message is capped and older tool calls are listed as counts. `uncapped` sends everything. It enables nothing by itself. |
| Verifier replay | `mike verifier-replay --execute` (subcommand) | dry run | yes | Rebuilt past verifier inputs for the chosen arm, to OpenRouter, only with `--execute --out`; spend stops at `--max-usd` (default 15). |
| Refocus steerer | `steerer.refocus.enabled` (setting) | off | yes | Part of the steerer fleet; injects grounding messages on anchor drift. |
| Lesson-relation detection | `lesson_relations.enabled` (setting) + the `mike detect-lesson-relations` subcommand | off | yes | Stored lesson text, to a proposer model and an adjudicator model. |
| Embeddings | `embedding.enabled` | off | no | Lesson bodies, operator-turn chunks, session overviews and recall queries, to `embedding.url`. The default URL is `http://127.0.0.1:7997`, which is local — but the URL is operator-settable at runtime, and a remote or hosted one means the text leaves the machine. |
| Trace export | `otel_endpoint` (boot config) | unset | no | Tracing spans to your OTel collector. mike's spans carry ids, sizes and counts; no prompt or response body was found on a span attribute. |

Every OpenRouter call's full outbound body is recorded in the `llm.calls` table, so the
database is also the audit log of what was sent.

### Local features that are on by default

These do not rewrite model traffic by themselves. Embedding work and database resync use
the configured embedding and database endpoints, which can be remote.

| Feature | Toggle | What it does |
|---|---|---|
| Lesson worker | `lesson_worker` | Consolidates and garbage-collects stored lessons. Calls the embedding service when embeddings are enabled; otherwise entirely local. |
| Memory harvester | `harvester` | Parses `<mem:update>` tags out of completed turns and writes the lessons to Postgres. No LLM call. |
| Database resync | `db_resync` | Drains rows written to the local fallback database back into the primary. See [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md). |
| Retention sweep | `retention_sweep` | Periodically deletes sessions older than `default_ttl` (365 days). |
| Channel-link sweep | `channel_link_sweep` | Maintains the Remote Control channel↔session links. |
| Workspace capture registration | `workspace_capture_registration` | Registers observed sessions in the workspace roster used for cross-session messaging. |
| Fleet auto-start | `fleet_auto_start` | Starts a fleet as soon as its definition is complete. Off, it waits for `mike fleet start`. |
| Prompt assistant | `prompt_assistant` | The prompt assistant; its sign-in and limits are on `/settings/assistants`. |
| Semantic context | `semantic_context` | On, but each call is refused unless the embedder URL is loopback (see the table above). |
| tether serves self-identity | `client_serves_self_identity` | When a client such as the tether mod supplies a session's identity note, mike stops adding its own for that session. |

### Local features that are off by default

These are local like the ones above, and ship off so a deploy changes nothing until the operator switches them on after deploy.

| Feature | Toggle | What it does |
|---|---|---|
| Dossier builder | `dossier_builder` | Builds `memory.turn_facts` for each closed operator turn from captured data: the operator's next genuine message, where sidecar text starts, how the turn ended, checks run and session class. No LLM call and no HTTP request. |
| Change record | `change_record` | Keeps one record of what happened in each checkout a session works in: file changes with the session that made each and the content before, commits, pushes, checks, verdicts, notes, mail and briefs, each with its paths. Fed by file notifications from the operating system, with a slow backstop. Lets `what_changed` answer without git. No LLM call and no HTTP request; nothing leaves the machine. See [`CHANGE_RECORD.md`](CHANGE_RECORD.md). |
| Change record lines | `change_record_tail` | Adds at most twelve lines under `since_you_last_looked` to the answers of `fleet_commit`, `fleet_check`, `write_session_note` and `send_session_message`, and to the first request of a turn as a reminder, saying what others changed in the files the session owns or has read. Needs `change_record`. With it off the answers are unchanged. |
| Directive proposer | `directive_proposer` | Proposes standing directives from directives the operator restated across sessions and hand-backs he confirmed, by SQL over captured data. Proposals wait on `/memory`; nothing is injected. No LLM call and no HTTP request. |

Also off by default, and described on the settings page (`featureCopy.ts`):

| Toggle | What it does |
|---|---|
| `workspace_briefing` | Adds the session's peers and its own name, role and goal to its requests. |
| `native_peer_mail` | Delivers mail from another session the way Claude Code delivers a message from one of your other sessions. Off, mail arrives as a mike channel message. |
| `mod_lesson_delivery` | Lets the mike mod add lessons when a prompt is sent; mike stops adding lessons to a session once it holds tagged ones. One session in five keeps mike adding them, so the two ways can be compared. |
| `context_focus_question` | Asks the model to add one short line to every tool call except file edits, saying what it wants; mike removes the field before Claude Code sees the call. |
| `launch_brief` | Delivers a spawned session's long brief as mail from mike instead of a file. |
| `judge_lessons`, `session_end_shadow` | Further judging jobs that call a model; they send captured text to your provider when on. |
| `client_serves_*` (ten switches other than `client_serves_self_identity`) | Each tells mike to stand aside for one stage on a session when a client such as tether does that stage itself. |

## Things that are not feature toggles

Four behaviours do not appear in the toggle list and are worth knowing about.

**Filter rules and lazy sections have no toggle.** Their only gate is whether you have authored
a rule that matches the project. If you have, the rule rewrites the `system[]` blocks of the
outbound request; the first-turn rewrite is captured and re-applied to every later turn of the
session until the ruleset fingerprint changes. Manage them at `/filter/workbench` or through
the `*_filter_rule` MCP tools. No rules means no rewrite.

**`reminder_on_token` changes the body in both positions.** The stage always runs. When a hook
sentinel token is present in the request, the toggle *on* replaces that block with a resolved
`<system-reminder>`, and the toggle *off* still strips the token block. It is fully inert only
when no such token is present — which is the case unless you installed a hook that emits one.

**The steerer drain is not gated by the `steerer` toggle.** The drain runs on primary and
first-primary turns, including queued mailbox deliveries.
The `steerer` toggle still gates steerer-origin reminders, along with enrolment and freshness
checks; it does not disable an explicitly queued mailbox message. A same-request LLM judge
can run when an OpenRouter key is
present *and* a steerer is enrolled for the session, independent of the toggle. With no key and
no enrolments — the default — neither happens.

**Out-of-band delivery bypasses the request body.** When a session has a live Remote Control
channel, a queued reminder can be pushed into that stream as a synthetic user turn instead of
being spliced into the next `/v1/messages` body. The model sees the same text; the captured
request bytes are unchanged. This is the `DeliveredOutOfBand` outcome in the reminder stages.

## See also

- [`SECURITY.md`](SECURITY.md) — the trust boundary, and how to confirm nothing is enabled
- [`GETTING_STARTED.md`](GETTING_STARTED.md) — stand up the stack before using any of the above
- [`CONFIGURATION.md`](CONFIGURATION.md) — boot configuration and every `MIKE_*` variable
- [`REQUEST_FLOW.md`](REQUEST_FLOW.md) — the runtime request flow across both binds
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — design rationale for each mechanism above
