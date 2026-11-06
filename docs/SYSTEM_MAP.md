---
title: System map
status: active
created: 2026-11-06
updated: 2026-11-06
summary: The one page to read first. Everything mike can do, where each ability lives in the code, which surface exposes it (CLI, REST, MCP, web UI, terminal UI), its default, and which doc goes deeper.
---

# System map

This is the entry point for a reader, human or model, who has just pulled the
repository and wants a complete and accurate read of the system. It was derived
from the code on the date above, not from the other docs. Every claim names the
file that proves it, and the [verification](#verify-this-page) section lists the
commands that print the authoritative lists, so a stale line here is cheap to
catch.

Reading order: this page, then [`ARCHITECTURE.md`](ARCHITECTURE.md) for the
reasons, [`REQUEST_FLOW.md`](REQUEST_FLOW.md) for one turn end to end,
[`FEATURES.md`](FEATURES.md) for every switch, and the
[docs index](#documentation-index) at the end for the rest.

## What mike is

mike is a reverse proxy for Claude Code's traffic to `api.anthropic.com`. It
records every request and response on your machine, shows them in a web UI,
lets Claude Code query that history over MCP, and can (only when switched on)
change what is sent and what comes back: lessons from earlier sessions, a nudge
when a session drifts, a rewrite of the system prompt, shortened tool output.
It also starts and coordinates Claude Code sessions (spawn, fleets, mail, work
threads) and runs a learning loop that turns corrections into lessons and
skills.

Three invariants hold everywhere:

- **Capture is the original bytes.** Every mutation stage changes the
  forwarded body; the stored copy is what the client sent
  (`crates/mike/src/proxy/engine/capture_register.rs`).
- **A fresh install only forwards.** Every stage that rewrites traffic or sends
  captured text to another service is off by default; what is on by default is
  local maintenance (`crates/mike/src/feature_toggles/mod.rs`,
  [`FEATURES.md`](FEATURES.md)).
- **No authentication, loopback only.** Any process on the machine can read
  everything and change settings; remote binds are refused unless an
  `unsupported_*` override is set ([`SECURITY.md`](SECURITY.md),
  [`LOCAL_ACCESS.md`](LOCAL_ACCESS.md)).

## Processes, binaries and ports

| Process | Bind | Binary / source | Role |
|---|---|---|---|
| Proxy | `127.0.0.1:9988` | `mike serve` (`crates/mike/src/proxy/`) | Accepts `/v1/*` and `/v2/*`, runs the pipeline, forwards to `upstream_url`, captures. |
| Inspect API | `127.0.0.1:9989` | same binary (`crates/mike/src/api/`) | `/api/*` REST and `/api/ws`. Read by the SPA and `mike-mcp`. |
| Web UI | `127.0.0.1:9990` | same binary serves the built SPA; `vite dev` in a checkout (`frontend/`) | SvelteKit single-page app, SSR off. |
| Forward sidecar | `127.0.0.1:9987` | `proxy-forward` (`crates/proxy-forward/`), also `mike forward` | HTTPS MITM forward proxy so Claude Code keeps Remote Control while mike captures; fails over to Anthropic while mike restarts. |
| MCP companion | stdio | `mike-mcp` (`crates/mike-mcp/`) | Spawned by Claude Code; talks HTTP to the inspect bind. Never links `mike` as a library. |
| Prompt assistant | `127.0.0.1:9985` | `services/prompt-assistant/` (Node) | Rewords captured system prompts with the Claude Agent SDK or Codex. Backend of `/settings/assistants`. |
| Terminal UI | tty | bare `mike` on a terminal (`crates/mike/src/tui/`) | Home, config, setup wizard, start/stop, sessions, requests. Bare `mike` off a tty runs `serve`. |
| Database | `:6666` owned Docker, `:5440` dev capture DB, `:6667` test DB | PostgreSQL 19 with pgvector (`crates/mike/resources/postgres19.Dockerfile`) | Primary store. Memory-only "trial" mode exists. |
| Claude Code mods | in Claude Code | `mods/` | Panes, status line, chips and skills inside Claude Code (`mike`, `fleet-comms`, `mike-mail`, `mods`, `thread`, `wiretap`). |
| Dev-only | `:6006` Storybook, `:9986` LikeC4 diagrams | `scripts/systemd/mike-stack-*.service.in` | Started by `scripts/start-proxy.sh`; never by a release install. |

Installed services: `mike service install` writes `mike.service` and
`mike-forward.service` (systemd user units) or `ai.temet.mike.proxy` and
`ai.temet.mike.forward` (launchd) (`crates/mike/src/service/`).

## Verify this page

These print the lists this page summarises. Prefer them over any doc when they
disagree.

| What | Command or file |
|---|---|
| CLI command tree | `mike --help`, `mike <cmd> --help`; `crates/mike/src/cli.rs` and `crates/mike/src/config/subcommands/mod.rs` |
| MCP tools and profiles | `mike-mcp list-tools --core --json`, `--full --json`; `crates/mike-mcp/src/tools/catalogue.rs`, `crates/mike-mcp/src/profiles.rs` |
| REST routes | `crates/mike/src/api/manifest/*.rs` (one file per area, each route with its MCP coverage) |
| Pipeline stage order | `pipeline::STAGE_ORDER` in `crates/mike/src/pipeline/mod.rs` |
| Feature toggles and defaults | `crates/mike/src/feature_toggles/mod.rs`; live: `GET /api/settings/features` |
| Presets | `crates/mike/src/feature_presets/plan.rs` |
| Background workers | `crates/mike/src/serve/workers.rs` (one `spawn_*` per worker) |
| Scheduled jobs | `crates/mike/src/scheduled_jobs/jobs.rs`; live: `GET /api/scheduled-jobs` |
| Web UI routes | `frontend/src/routes/` (file routing); nav in `frontend/src/lib/components/shell/DestinationNav.svelte` |
| Boot config and env | `mike print-config`; `crates/mike/src/config/schema.rs`, `config/proxy_config_defaults.rs` |
| Runtime settings | `GET /api/settings`; `crates/mike/src/settings/meta_*.rs` |
| Schema | `crates/mike/src/store/persistence/migrations/` (312 files); [`SCHEMA.md`](SCHEMA.md) documents 146 tables and is behind on 79 |
| Doc drift gate | `scripts/rule-checks/judgement-context-docs.sh` (ledgers in `judgement-context-docs.d/`) |
| User guide vs CLI | `crates/mike/tests/cli/user_guide.rs`, `scripts/check-user-guide --complete` |

## Capabilities

Each row: what it does, where the code is, which surfaces expose it, its
default, and the doc that goes deeper. "Session toggle" means a new session
freezes the value at its first request; "global" means it applies live.

### Capture and inspection (always on)

| Capability | Code | Surfaces | Doc |
|---|---|---|---|
| Capture every request and streamed response, two-phase (stub row on accept, full snapshot on completion) | `proxy/engine/`, `sse/tee.rs`, `store/persistence/` | UI `/sessions/<id>`, `/sessions/<id>/requests/<id>`; REST `/api/requests/*`; MCP `list_requests`, `get_request`, `get_request_body`, `get_response_body`, `get_accumulated_text` | [`REQUEST_FLOW.md`](REQUEST_FLOW.md) |
| Record what changed before forwarding (transformation manifest, sent body) | `pipeline/`, `store/persistence/` | REST `/api/requests/{id}/manifest`, `/body/request_sent`; MCP `get_request_sent_body`, `get_request_transformation_manifest` | [`REQUEST_FLOW.md`](REQUEST_FLOW.md) |
| Group requests into sessions, projects, operator turns and agent dispatches | `store/`, `agents/`, `capture/` | UI `/`, `/projects`, `/sessions`, `/agents`, `/diagnostics/agent-dispatches`; MCP `list_sessions`, `get_session`, `get_session_overview`, `list_operator_turns`, `list_agent_dispatches` | [`OPERATOR_TURN_ATTRIBUTION.md`](OPERATOR_TURN_ATTRIBUTION.md) |
| Live updates over WebSocket (text deltas, tool use, row changes, steering fired) | `api/ws.rs`, `store/models/proxy_event.rs`, `change_feed/`, `db_listener/` | `GET /api/ws` | [`ARCHITECTURE.md` §13](ARCHITECTURE.md#13-web-ui-architecture) |
| Text search over bodies and conversations; tool-error feed; file-write collisions | `api/handlers/`, `store/persistence/` | UI `/tools`, search box; MCP `find_text`, `search_session_content`, `list_tool_errors`, `list_file_writes` | user guide [reading-sessions](user-guide/reading-sessions.md) |
| Flag weak answers: 19 regex patterns in 5 categories, local only | `analysis/` | UI `/analysis`, `/settings/flagging`; REST `/api/diagnostics/*`; MCP `diagnostic_stats`, `diagnostic_findings` | [`FEATURES.md`](FEATURES.md#response-analysis-and-optional-diagnostic-reminders) |
| System prompt classes, versions, volatile lines and anomalies | `compose_prompt/`, `store/persistence/` | UI `/prompts/*`; REST `/api/prompts/*`; MCP `list_prompt_compositions`, `list_prompt_anomalies` | [`ARCHITECTURE.md` §4.4](ARCHITECTURE.md#44-prompt-composition) |
| Session notes, goals and time spent | `api/handlers/`, `work_items/` | MCP `write_session_note`, `list_session_notes`, `set_session_goal`, `get_time_spent` | [`MCP_BRIDGE.md`](MCP_BRIDGE.md) |
| Remote Control capture through the sidecar, with out-of-band injection into a live channel | `proxy/remote_control/`, `crates/proxy-forward/` | `mike claude-code route sidecar`, `mike forward` | [`FORWARD_PROXY_WEBSOCKET_UPGRADE.md`](FORWARD_PROXY_WEBSOCKET_UPGRADE.md), [`INJECTION_TRUST.md`](INJECTION_TRUST.md) |
| Health, capabilities, stats, metrics, DB cost, memory pressure | `api/handlers/health/`, `metrics/`, `db_cost/`, `memory_cap/` | REST `/api/health`, `/api/capabilities`, `/api/stats`, `/api/metrics`, `/api/db-cost`; MCP `get_stats`; `proxy://status` resource | [`CAPTURE_OUTAGES.md`](CAPTURE_OUTAGES.md), [`CAPTURE_RETAINED_BYTES.md`](CAPTURE_RETAINED_BYTES.md) |

### Change what is sent: the outbound pipeline

Stages run in this order on every captured request (`pipeline/mod.rs`
`STAGE_ORDER`; `proxy/engine/mutation_stages.rs`). Earlier injections are
replayed first so the prompt cache prefix stays stable (`injected_replay/`).
"Own lane" is a primary turn with no subagent id.

| # | Stage | Gate (default) | What it does | Doc |
|---|---|---|---|---|
| 1 | Filter rules | a rule matches the project (none shipped) | Rewrites `system[]` blocks by authored rules; the first-turn rewrite is replayed on later turns | user guide [filters](user-guide/filters.md) |
| 2 | Reminder on token | `reminder_on_token` (off); always strips the sentinel | Replaces a hook sentinel with a resolved reminder | [`FEATURES.md`](FEATURES.md) |
| 3 | Lazy sections | lazy filter rules exist | Replaces prompt sections with stubs the model can expand | [`ARCHITECTURE.md` §4.2](ARCHITECTURE.md#42-lazy-prompt-sections) |
| 4 | Recall injector | `recall_injector` (off) | Topic-matched lessons from the database | user guide [memory](user-guide/memory.md) |
| 5 | Session lessons | setting `reminders.inject_lessons` (off), own lane | Session-start lessons on the first turn | [`IMPROVEMENT_LOOP.md`](IMPROVEMENT_LOOP.md) |
| 6 | Self identity | `self_identity_reminder` (off) | Who this session is | [`INJECTION_TRUST.md`](INJECTION_TRUST.md) |
| 7 | Workspace briefing | `workspace_briefing` (off) | Peers, own name, role and goal | [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md) |
| 8 | Agent lessons | `agent_lessons` (off), subagent turns | Persona-scoped lessons | user guide [memory](user-guide/memory.md) |
| 9 | Diagnostic reminders | `reminders` (off), own lane | Findings on the previous response as a `<system-reminder>` | [`FEATURES.md`](FEATURES.md) |
| 10 | Change record tail | `change_record_tail` (off) | What others changed in files this session owns | [`CHANGE_RECORD.md`](CHANGE_RECORD.md) |
| 11 | Git read hint | `change_record` (off) | Nudge to use `what_changed` instead of git | [`CHANGE_RECORD.md`](CHANGE_RECORD.md) |
| — | Steerer drain | not gated here; producers are | Delivers queued steerer interventions and mailbox messages, up to `mailbox_max_deliveries_per_turn` | [`STEERING_EXTENSION.md`](STEERING_EXTENSION.md) |
| 12 | Repeated notices | `repeated_notices` (off) | Sends each Claude Code boilerplate notice once per session | [`FEATURES.md`](FEATURES.md) |
| 13 | Read dedup | setting `compression_dedup.enabled` (off) | Replaces duplicate Read results | user guide [compression](user-guide/compression.md) |
| 14 | File-read compression | `file_read_compression` (off); mode `shadow` / `forward`, `forward_share` | Head and tail of older large reads, pointer to the captured original | user guide [compression](user-guide/compression.md) |
| 15 | Shell-output compression | `shell_output_compression` (off) | Older Bash output to head, tail and signal lines | user guide [compression](user-guide/compression.md) |
| 16 | Proxy-declared tools | `proxy_tools` (off) | Adds mike's own tools (`mike_session_roster`) to `tools[]` and answers them in-process | [`ARCHITECTURE.md` §4.5](ARCHITECTURE.md#45-weasel-intercept) |
| 17 | Context-focus question | `context_focus_question` (off) | Asks for one line per tool call; stripped from the response | [`FEATURES.md`](FEATURES.md) |

After the stages: prompt composition (`compose_prompt/`; `composition_enabled`
exists in config but the stage does not read it), a cache-scope tripwire
(`proxy/engine/guards.rs`), capture admission, then forwarding. Each
`client_serves_*` toggle (11 keys, `client_serves_self_identity` on) tells mike
to stand aside for one stage when the Claude Code mod does it itself
([`ARCHITECTURE.md` §4.7](ARCHITECTURE.md#47-client-served-stages-442)).

### Change what the client receives

| Capability | Gate (default) | Code | Doc |
|---|---|---|---|
| Weasel intercept: cut a streamed answer before a forbidden word, re-ask with a correction, continue the same message | `weasel_intercept` (off) and a non-empty word list | `weasel_intercept/`, `proxy/response/streaming.rs` | [`ARCHITECTURE.md` §4.5](ARCHITECTURE.md#45-weasel-intercept) |
| Proxy-tool loop: hold a call to a proxy-declared tool, answer it from the database, splice the result, continue | `proxy_tools` (off) | `proxy_tools/`, `weasel_intercept/proxy_tool.rs` | [`REQUEST_FLOW.md`](REQUEST_FLOW.md) |
| Memory harvester: `<mem:update>` tags in a response become lessons | `harvester` (on) | `memory_harvester/`, `store/updater/harvester_hook.rs` | user guide [memory](user-guide/memory.md) |
| Out-of-band delivery into a live Remote Control channel | a channel is linked | `proxy/remote_control/inject.rs` | [`INJECTION_TRUST.md`](INJECTION_TRUST.md) |

### Judge, steer and learn (background workers)

All workers are spawned by `serve/workers.rs` and need persistence. The ones
that call a model are inert without an OpenRouter key
(`MIKE_STEERER_OPENROUTER_API_KEY` or `OPENROUTER_API_KEY`) or a configured
provider (`mike setup models`, `/api/llm/providers`). Every model call is
recorded in `llm.calls`.

| Worker | Gate (default) | Calls a model | What it does | Surfaces | Doc |
|---|---|---|---|---|---|
| Steerer fleet | `steerer` (off) | yes | Enrolled steerers watch sessions for goal and anchor drift and queue `<system-reminder>` interventions; verifier and refocus steerers. Verifier input is capped per section by `steerer.verifier.input_budget` | UI `/steering`, `/steering/<id>`, `/settings/steering`; REST `/api/steerers/*`; MCP `get_verifier_rate`, `list_steerer_evaluations` | [`STEERING_EXTENSION.md`](STEERING_EXTENSION.md), user guide [steering](user-guide/steering.md) |
| Quiescence worker | `steerer` (off) | via its ticks | Steerer ticks after quiet periods; label ticks; guard demotion; lesson proposer tick when `judge_lessons` is on; turn guard when `turn_guard` is on | — | `quiescence_worker/` |
| Turn judge | `judge_worker` (off) | yes | Labels and verdicts per operator turn | UI `/sessions/labelling`, `/sessions/labelling/accuracy`; MCP `get_turn_verdicts`, `list_eval_labels`, `relay_label` | user guide [judge](user-guide/judge.md) |
| Session judge, session-end shadow | `session_judge`, `session_end_shadow` (off) | yes | Outcome and quality per session | MCP `get_session_verdicts`, `list_eval_session_end_items` | [`ARCHITECTURE.md`](ARCHITECTURE.md#decision-session-judge-per-session-outcomequality--issue-24) |
| Turn guard | `turn_guard` (off) | yes | Four guard questions at turn end; each starts in `shadow`, promoted to `append` or `wake` by the operator | UI `/sessions/improvement/guard`; REST `/api/steering/guard/*`; `mike guard …` | [`IMPROVEMENT_LOOP.md`](IMPROVEMENT_LOOP.md) |
| Lesson worker | `lesson_worker` (on) | no (embeddings if enabled) | Consolidates, dedups and garbage-collects lessons; nightly consolidation | UI `/memory`; MCP `record_lesson`, `list_lessons`, `delete_lesson` | user guide [memory](user-guide/memory.md) |
| Lesson proposer | `judge_lessons` (off) | yes | Proposes lessons from judged turns; proposals wait on `/proposals` and `/memory` | UI `/proposals`, `/skills`; REST `/api/directive-proposals`, `/api/skills/*` | [`IMPROVEMENT_LOOP.md`](IMPROVEMENT_LOOP.md), [`LEARNING_OPERATIONS.md`](LEARNING_OPERATIONS.md) |
| Dossier builder | `dossier_builder` (off) | no | `memory.turn_facts` per closed turn: next genuine message, how it ended, checks run | MCP `get_session_dossier`, `get_turn_facts`; `mike dossier-*` | [`ARCHITECTURE.md` §15](ARCHITECTURE.md#15-operator-turn-lifecycle) |
| Semantic context | `semantic_context` (on, loopback embedder only) | embedder | Semantic items, pattern tagging and grouping, drift | REST `/api/semantic/*`; MCP `get_semantic_*`; `mike semantic …` | [`ARCHITECTURE.md` §15](ARCHITECTURE.md#semantic-context) |
| Embed worker and knowledge scan | `embedding.enabled` (off) | embedder at `embedding.url` | Embeds turns, sessions and knowledge chunks; scans `~/.claude` memory and skills into knowledge sources every 120 s | UI `/settings/embedding`; MCP `recall_similar_turns`, `search_project_knowledge` | [`PROJECT_KNOWLEDGE.md`](PROJECT_KNOWLEDGE.md) |
| Eval metrics | always | no | Daily rollups, rates and alarms over judge output | REST `/api/eval/*`; `mike eval …`; MCP `get_eval_rates`, `list_eval_alarms` | [`IMPROVEMENT_LOOP.md`](IMPROVEMENT_LOOP.md) |
| Directive proposals | `directive_proposer` (off) | no | Proposes standing directives from corrections the operator restated across sessions, by SQL over captured data; also calibrates guard thresholds across sessions. Proposals wait on `/memory`; nothing is injected | REST `/api/cross-session/*`; MCP `get_cross_session_calibration` | [`ARCHITECTURE.md` §15](ARCHITECTURE.md#cross-session-calibration-and-directive-proposals) |
| Change record | `change_record` (off) | no | Watches checkouts for file changes, commits, pushes and checks so `what_changed` answers without git | MCP `what_changed`; REST `/api/changes` | [`CHANGE_RECORD.md`](CHANGE_RECORD.md) |
| Retention sweep, mailbox reaper, workspace reaper, channel-link sweep, work registry, health refresher, DB cost monitor, resync | `retention_sweep`, `channel_link_sweep`, `db_resync` (on); the rest always | no | Local housekeeping: TTL deletes (365 d default), expired mail, lapsed roster rows, channel links, DB statement budgets, fallback-to-primary resync | REST `/api/sync/status`, `/api/db-cost` | [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md) |
| Scheduled jobs | per-job `<job>.enabled` (on) | the scripts may | Runs repo scripts on a schedule: `learning_promote`, `learning_audit`, `compressor_loop`, `cargo_sweep`, `lessons_commit`, `usage_audit` | REST `/api/scheduled-jobs`, `POST …/{name}/run`; UI `/settings/workers` | [`LEARNING_OPERATIONS.md`](LEARNING_OPERATIONS.md) |

### Sessions, fleets, mail and threads (coordination)

| Capability | Code | Surfaces | Doc |
|---|---|---|---|
| Start Claude Code through mike with a session id, proxy env or sidecar route, folder trust | `claude.rs`, `claude_code/` | `mike claude`, `mike claude-code status\|mcp\|route\|shell\|mods\|trust` | user guide [command-line](user-guide/command-line.md) |
| Spawn sessions into terminal surfaces (tab, split, panel) with name, role, goal, instructions and a brief; warm templates and fork | `launch/` (backends `herdr`, `herdr_core`, `tmux`, `terminal`, `none`; `ghostty` is a reserved name with no backend yet) | `mike spawn open\|list\|kill\|doctor\|warm\|fork\|templates\|reap`; REST `/api/launches/*`; MCP `spawn_session`, `list_launches`, `kill_session` | [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md), user guide [sessions](user-guide/sessions.md) |
| Fleets: several seats working one goal with tasks, checks, verdicts, a record of the run | `launch/fleet/`, `.claude/fleet/` (rules, roles, leaves) | `mike fleet …`, `mike discover`; REST `/api/fleets/*`; MCP `define_fleet`, `get_fleet`, `fleet_commit`, `fleet_check`, `fleet_report`, `fleet_verdict`, `fleet_decide`, `fleet_propose`, `fleet_split`; UI `/fleet`; mod `fleet-comms`; toggle `fleet_auto_start` (on) | [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md), [`FLEET_EVENTS.md`](FLEET_EVENTS.md), [`FLEET_ISOLATED_CHECKS.md`](FLEET_ISOLATED_CHECKS.md), user guide [fleets](user-guide/fleets.md) |
| Workspace roster: which sessions are live in a project, heartbeats, channel claims | `workspace/`, `workspace_reaper.rs` | REST `/api/workspace/*`; MCP `get_workspace_roster`, `get_fleet_graph`; toggle `workspace_capture_registration` (on) | [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md) |
| Mail between sessions, threads, questions and answers; delivery as a reminder, as native peer mail, or over a Claude Code channel | `store/persistence/mailbox/`, `mcp_channel.rs`, `mailbox_reaper.rs` | MCP `send_session_message`, `get_session_messages`, `ask`, `answer`, `decline`, `read_thread`; REST `/api/sessions/{id}/message*`, `/api/threads/*`; UI `/comms`, `/sessions/<id>/comms`; toggles `native_peer_mail`, `mailbox_redelivery` (off); mod `mike-mail` | [`INJECTION_TRUST.md`](INJECTION_TRUST.md), user guide [reading-sessions](user-guide/reading-sessions.md) |
| Work threads: work that outlives a session, with checkpoints and dependencies | `work_threads/` | `mike thread import`; REST `/api/work-threads/*`; MCP `work_thread_list\|get\|checkpoint\|add_entry`; mod `thread` | user guide [threads](user-guide/threads.md) |
| Project knowledge: projects, locations, knowledge sources (memory files, skills), hybrid search | `knowledge_scan/`, `api/handlers/knowledge*`, `scripts/knowledge/` | REST `/api/knowledge/*`, `/api/projects/*`; MCP `define_project`, `register_project_location`, `search_project_knowledge`, `read_project_knowledge`; UI `/projects/<path>` | [`PROJECT_KNOWLEDGE.md`](PROJECT_KNOWLEDGE.md), user guide [projects](user-guide/projects.md) |
| Skills: drafts from accepted lessons, policy, install and retire, live outcomes | `lesson_proposer/`, `scripts/learning-promote` | REST `/api/skills/*`; UI `/skills` | [`LEARNING_OPERATIONS.md`](LEARNING_OPERATIONS.md) |
| Claude Code mods: sessions pane, mail on the status line, `/recall`, fleet pane, `/mods`, thread skills, wiretap | `mods/` | `mike claude-code mods [--with-wiretap]` | [`mods/README.md`](../mods/README.md), user guide [mods](user-guide/mods.md) |

### Setup, operation and repair

| Capability | Code | Surfaces | Doc |
|---|---|---|---|
| Guided setup: welcome, machine, database, service, claude, features, models, embeddings, terminal, ready | `setup/`, `tui/screens/setup/` | `mike setup [step]`, `--mode trial\|docker\|existing`; UI `/settings/setup` | user guide [setup](user-guide/setup.md) |
| Feature presets | `feature_presets/` | `mike features watch-only\|recommended\|everything` | [`FEATURES.md`](FEATURES.md#three-starting-points-mike-features) |
| Database: provision an owned Docker PostgreSQL 19, check an existing one, native install | `setup/cli.rs`, `db_provision/` | `mike database provision\|check\|stop`, `mike db up\|down\|status\|check\|native` | [`INSTALLED_DATABASE.md`](INSTALLED_DATABASE.md) |
| Local fallback database with resync to the primary | `config/persist_bootstrap.rs`, `resync/` | boot keys `local_database_url`, `db_fallback_enabled` | [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md) |
| Health checks, support bundle | `doctor/` | `mike doctor [--json] [--support-bundle]` | user guide [troubleshooting](user-guide/troubleshooting.md), [`PILOT_FEEDBACK.md`](PILOT_FEEDBACK.md) |
| Background service | `service/` | `mike service install\|status\|restart\|uninstall` | user guide [command-line](user-guide/command-line.md) |
| MCP registration in Claude Code | `setup/mcp/` | `mike mcp probe\|register\|unregister`, `mike claude-code mcp` | user guide [mcp](user-guide/mcp.md) |
| Config: file, env, CLI layers; live settings overlay with history | `config/`, `settings/`, `user_config/` | `mike print-config [--shell\|--paths]`, `mike config path\|import-env`, `mike import-dotenv`; REST `/api/settings*`, `/api/config/change-log`; UI `/settings/*`, `/settings/history` | [`CONFIGURATION.md`](CONFIGURATION.md), user guide [settings](user-guide/settings.md) |
| Migrations: applied only by `mike serve`; other commands refuse on a mismatch | `store/persistence/migrations/` | `mike migrate-check`, `mike forget-retracted-migrations` | [`DEVELOPMENT.md`](DEVELOPMENT.md) |
| Backfills and repairs for older captures; space reclaim | `config/subcommands/` | `mike backfill-*`, `reclassify-system-prompts`, `repair-session-prompts`, `restamp-sidecar-origins`, `demote-duplicate-first-turns`, `compact-request-bodies`, `reclaim-space`, `sweep-orphaned-messages` | user guide [command-line](user-guide/command-line.md#repair-and-analysis-commands) |
| Research and tuning: eval runs and gates, semantic backtests, guard reports, steer questions, directives, verifier replay (spends money with `--execute`) | `config/subcommands/{eval,semantic,guard_report,steer_questions,directives,verifier_replay}.rs`, `eval/` | `mike eval …`, `mike semantic …`, `mike guard …`, `mike steer-questions …`, `mike directives …`, `mike verifier-replay`, `mike export-analysis-corpus` | [`IMPROVEMENT_LOOP.md`](IMPROVEMENT_LOOP.md), [`SECURITY.md`](SECURITY.md) |
| Backup, restore, upgrade check, uninstall | `uninstall/`, `scripts/release/recovery.py` | `mike uninstall [--purge]`, `scripts/release-maintenance` | [`RECOVERY.md`](RECOVERY.md), user guide [backup-and-recovery](user-guide/backup-and-recovery.md) |

## Feature toggles at a glance

Defined in `crates/mike/src/feature_toggles/mod.rs`; changed with
`PATCH /api/settings/features/{key}` or on `/settings`. A stored value always
wins over a code default, so an upgrade never changes what an install does.

- **On by default (local only):** `lesson_worker`, `harvester`, `db_resync`,
  `retention_sweep`, `channel_link_sweep`, `workspace_capture_registration`,
  `fleet_auto_start`, `prompt_assistant` (read only by the SPA),
  `semantic_context` (refuses a non-loopback embedder),
  `client_serves_self_identity`.
- **Off by default, rewrite traffic:** `reminders`, `recall_injector`,
  `agent_lessons`, `self_identity_reminder`, `workspace_briefing`,
  `mailbox_redelivery`, `repeated_notices`, `reminder_on_token`,
  `file_read_compression`, `shell_output_compression`, `proxy_tools`,
  `context_focus_question`, `change_record_tail`, `weasel_intercept`, and the
  settings `reminders.inject_lessons`, `compression_dedup.enabled`.
- **Off by default, call a model:** `steerer`, `judge_worker`,
  `session_judge`, `session_end_shadow`, `turn_guard`, `judge_lessons`.
- **Off by default, local:** `dossier_builder`, `directive_proposer`,
  `change_record`, `launch_brief`, `native_peer_mail`, `mod_lesson_delivery`,
  ten `client_serves_*` keys.

Presets (`feature_presets/plan.rs`): **watch-only** is every default;
**recommended** adds `recall_injector`, `agent_lessons`, `reminders` and
`reminders.inject_lessons`; **everything** turns every switch on.

## The `mike` command tree

Root: `crates/mike/src/cli.rs`; the maintenance commands are flattened from
`config/subcommands/mod.rs`. Full flags: `mike <cmd> --help`; the user guide's
[command line](user-guide/command-line.md) page documents each and is tested
against the binary.

```
mike                       TUI on a terminal, `serve` otherwise
mike serve | doctor | setup [step] | features <preset> | uninstall [--purge]
mike claude [args]         start Claude Code through mike
mike claude-code status|mcp|route|shell|mods|trust
mike mcp probe|register|unregister
mike database provision|check|stop        mike db up|down|status|check|native
mike service install|status|restart|uninstall
mike forward [serve|export-ca|ca-path|print-config]
mike spawn open|list|kill|doctor|warm|fork|templates|reap
mike fleet status|list|start|stop|summary|focus|commit|check|where|gate|record
mike discover | thread import
mike config path|import-env    mike import-dotenv | print-config | migrate-check
mike backfill-* | reclassify-system-prompts | repair-session-prompts
mike restamp-sidecar-origins | demote-duplicate-first-turns | origin-backfill
mike compact-request-bodies | reclaim-space | sweep-orphaned-messages
mike forget-retracted-migrations | detect-lesson-relations
mike eval … | semantic … | guard … | steer-questions … | directives …
mike verifier-replay | dossier-backfill | dossier-parity | dossier-render
mike export-analysis-corpus --out
```

`mike-mcp [serve|list-tools] [--<profile>]`; `proxy-forward serve|export-ca|print-config`.

## MCP companion

`crates/mike-mcp/` exposes 126 tools (`tools/catalogue.rs`) over stdio, each
with a `select` projection, plus the resources `proxy://status`,
`proxy://requests`, `proxy://sessions`, `proxy://requests/{id}`,
`proxy://sessions/{id}`. Profiles (`profiles.rs`): `core` (the default: what
you reach for while investigating a session), `full`, and the seat profiles
`worker`, `coordinator`, `acceptance`, `observer`, `retrospective`,
`reference`, `problem_solver`. Set with `MIKE_MCP_PROFILE` or `--<profile>`;
`MIKE_INSPECT_URL` points it at the inspect bind. Tools are added only when a
workflow needs one; `crates/mike-mcp/tests/manifest_coverage.rs` rejects a tool
without a route and `crates/mike/src/api/manifest/*.rs` marks every route
`Exempt` or covered. [`MCP_BRIDGE.md`](MCP_BRIDGE.md) has the families and the
seat contracts; `mike-mcp list-tools --full --json` is the catalogue.

## Inspect API

`crates/mike/src/api/manifest/` holds one file per area; each row names the
route, the handler and its MCP coverage. Areas: `requests`, `sessions`,
`session_mail`, `memory`, `lesson_delivery`, `projects`, `knowledge`,
`filters`, `prompts`, `prompt_shaping`, `compression_review`, `steering`,
`evaluations`, `steer_questions`, `verdicts`, `dossier`, `eval`,
`eval_metrics`, `semantic`, `cross_session`, `improvement`, `skills`,
`skill_outcomes`, `proposals`, `fleets`, `launches`, `workspace`, `questions`,
`work_threads`, `changes`, `config_history`, `anthropic`, `setup`,
`websocket`. The inspect router has a 30 s timeout; a host guard refuses
non-loopback `Host` headers unless `inspect_allowed_hosts` names them
(`api/host_guard.rs`).

## Web UI

SvelteKit, file routing under `frontend/src/routes/`, SSR off. The left menu
groups pages as Overview, Levers, Insight and System; the user guide's
[Find a page](user-guide/pages.md) maps every route to its guide page.

- **Overview:** `/`, `/projects[/<path>[/graph]]`, `/sessions`,
  `/sessions/<id>[/graph|/comms|/requests/<id>]`, `/comms`,
  `/sessions/improvement[/guard|/lessons/<id>]`,
  `/sessions/labelling[/accuracy[/<run>[/review]]]`.
- **Levers:** `/filter/workbench`, `/steering[/<id>]`.
- **Insight:** `/analysis`, `/prompts[/main|/new|/subagent/<persona>|/volatile-lines]`,
  `/agents[/<id>]`, `/fleet`, `/compression-review`, `/tools`,
  `/diagnostics/agent-dispatches`.
- **System:** `/memory`, `/skills[/<id>]`, `/proposals`, `/diagnostics/memory`,
  `/settings[/steering|/workers|/assistants|/embedding|/flagging|/integrations|/history|/setup]`,
  `/help[/guides|/glossary|/docs/<slug>]`.
- Redirects: `/steerers` → `/steering?tab=fleet`, `/judges/*` and
  `/judge-disagreements/*` → `/sessions/labelling/*`.

Frontend conventions, component catalogue and route contracts:
[`docs/frontend/CONVENTIONS.md`](frontend/CONVENTIONS.md),
[`docs/frontend/COMPONENT_CATALOGUE.md`](frontend/COMPONENT_CATALOGUE.md),
[`docs/frontend/ROUTES.md`](frontend/ROUTES.md); Storybook's Foundations stories are
the visual source of truth.

## Terminal UI

`crates/mike/src/tui/`: Home (services and status; `s` setup, `l` start or
stop, `t` launch Claude, `r` refresh), Config (toggles and settings), Setup
(the wizard steps listed above), Start/Stop, Sessions, Requests and Request
detail. [`tui/STYLE_GUIDE.md`](tui/STYLE_GUIDE.md) and
[`tui/LAYOUT.md`](tui/LAYOUT.md) define it; user guide
[terminal](user-guide/terminal.md) describes using it.

## Storage

- **In-memory ring buffer** always runs and backs the live views
  (`store/mod.rs`, `store/retention.rs`; `max_requests`, `max_store_bytes`,
  `max_age`, memory-pressure eviction).
- **PostgreSQL 19 + pgvector** is the primary (`database_url`), with three pool
  classes (capture, interactive, background). Schemas: `requests`, `responses`,
  `sessions`, `system_prompts`, `prompt_kinds`, `tools`, `memory`, `steering`,
  `session_judges`, `eval`, `llm`, `mailbox`, `messages`, `workspace`, `work`,
  `launch`, `agents`, `remote_control`, `projects`, `rules`, `overrides`,
  `config`, `jobs`, `sync`, `runtime`. [`SCHEMA.md`](SCHEMA.md) describes most
  tables; the migrations folder is authoritative.
- **Local fallback** (`local_database_url`) with resync; **trial** mode keeps
  everything in memory.
- Retention: sessions older than `default_ttl` (365 days) are swept; pinned
  projects and sessions are kept.

## Configuration

Boot config: defaults < `~/.config/mike/config.toml` < `MIKE_*` env < CLI
flags (`config/load/mod.rs`). Env names are the key upper-cased with `__` for
nesting (`MIKE_EMBEDDING__ENABLED`); `MIKE_LISTEN_ADDR` and
`MIKE_INSPECT_ADDR` are the bind aliases. Runtime settings are a separate
database-backed overlay (`settings/`), changed with `PATCH /api/settings/{key}`
and shown by `mike print-config` under `LIVE OVERLAY`. Every key:
[`CONFIGURATION.md`](CONFIGURATION.md).

## What leaves the machine

Nothing but the forwarded request to `upstream_url`, until an operator turns on
a model-calling feature (OpenRouter or a configured provider), points
`embedding.url` or the semantic embedder off loopback, sets `otel_endpoint`, or
runs `detect-lesson-relations` or `verifier-replay --execute`. The ledger that
changes with the code is
[`WHAT_MIKE_STORES_AND_SENDS.md`](WHAT_MIKE_STORES_AND_SENDS.md); the user
narrative is [privacy](user-guide/privacy.md); the trust boundary is
[`SECURITY.md`](SECURITY.md).

## Known gaps and vestigial code

Facts a reader would otherwise rediscover the hard way.

- `crates/mike/src/runtime/codex/` and the `runtime.codex_*` tables are
  scaffolding for a managed Codex runtime with no production callers; fleets
  hard-code the Claude runtime. The Codex CLI as an LLM provider
  (`llm/codex`) is live.
- `composition_enabled` is read by the banner and `print-config` only; the
  composition stage does not consult it.
- `prompt_assistant` is a toggle only the SPA reads.
- `confirm_message_receipt`, `register_workspace_session` and
  `workspace_heartbeat` are in `CORE_TOOL_NAMES` but `FULL_ONLY_TOOLS` removes
  them, so they are full-only.
- The quiescence worker, and with it the lesson proposer, runs only while
  `steerer` is on.
- The doc drift gate (`scripts/rule-checks/judgement-context-docs.sh`) owns
  this page too: a `schema.table` named here must exist in a migration, and the
  ledgers under `judgement-context-docs.d/` require some fragments of it.
- The agent names in `AGENTS.md`'s routing table (`rust-developer`,
  `frontend-developer`, …) are user-level agents, not files in this repository;
  `.claude/agents/` holds only the two fleet agents.

## Documentation index

Start with the [user guide](user-guide/README.md) for anything a user does;
it is published to the site, checked by `scripts/check-user-guide`, and its
command and settings pages are tested against the binary.

**Design and flows (contributor)**

| Doc | Read it for |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Rationale, decisions, module map, the operator-turn lifecycle |
| [`../ARCHITECTURE.md`](../ARCHITECTURE.md) (root) | Developer quick start: conventions, test tiers, how to add code |
| [`REQUEST_FLOW.md`](REQUEST_FLOW.md) · [`STARTUP_FLOW.md`](STARTUP_FLOW.md) | One request through both binds; `mike serve` from boot to shutdown |
| [`architecture/README.md`](architecture/README.md) | The LikeC4 model, live on `:9986` in a checkout |
| [`animations/README.md`](animations/README.md) | Animated diagrams of a turn, the pipeline, the two-phase write and boot |
| [`SCHEMA.md`](SCHEMA.md) · [`RUNTIME_CONTRACT.md`](RUNTIME_CONTRACT.md) | Tables; the runtime identity and event contract |
| [`CAPTURE_OUTAGES.md`](CAPTURE_OUTAGES.md) · [`CAPTURE_RETAINED_BYTES.md`](CAPTURE_RETAINED_BYTES.md) | What happens when capture stalls; the byte budget of a live stream |
| [`FORWARD_PROXY_WEBSOCKET_UPGRADE.md`](FORWARD_PROXY_WEBSOCKET_UPGRADE.md) | The sidecar's MITM, upgrade and failover |
| [`OPERATOR_TURN_ATTRIBUTION.md`](OPERATOR_TURN_ATTRIBUTION.md) | Which turns count as the operator's own lane |

**Features and behaviour**

| Doc | Read it for |
|---|---|
| [`FEATURES.md`](FEATURES.md) | Every switch: default, what it rewrites, what it sends |
| [`CONFIGURATION.md`](CONFIGURATION.md) | Every boot key and `MIKE_*` variable |
| [`SECURITY.md`](SECURITY.md) · [`LOCAL_ACCESS.md`](LOCAL_ACCESS.md) · [`WHAT_MIKE_STORES_AND_SENDS.md`](WHAT_MIKE_STORES_AND_SENDS.md) | Trust boundary, listener policy, the stores-and-sends ledger |
| [`INJECTION_TRUST.md`](INJECTION_TRUST.md) | Which injected text a session may act on; read before designing an injection |
| [`STEERING_EXTENSION.md`](STEERING_EXTENSION.md) | Adding a steerer or a guard question |
| [`IMPROVEMENT_LOOP.md`](IMPROVEMENT_LOOP.md) · [`LEARNING_OPERATIONS.md`](LEARNING_OPERATIONS.md) | Judge → lessons → skills, and the jobs that run it |
| [`PROJECT_KNOWLEDGE.md`](PROJECT_KNOWLEDGE.md) | Projects, locations, knowledge sources and search |
| [`CHANGE_RECORD.md`](CHANGE_RECORD.md) | The per-checkout record behind `what_changed` |
| [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md) · [`FLEET_EVENTS.md`](FLEET_EVENTS.md) · [`FLEET_ISOLATED_CHECKS.md`](FLEET_ISOLATED_CHECKS.md) | Spawning, fleets, their event log and isolated checks |
| [`MCP_BRIDGE.md`](MCP_BRIDGE.md) | Tool families, profiles, seat contracts |
| [`decisions/`](decisions/) · [`adr/`](adr/) | Inspection fidelity, prompt shaping on the wire, comment conventions; one superseded ADR |

**Working from a checkout**

| Doc | Read it for |
|---|---|
| [`../AGENTS.md`](../AGENTS.md) | Project instructions: commands, limits, routing work |
| [`GETTING_STARTED.md`](GETTING_STARTED.md) · [`DEVELOPMENT.md`](DEVELOPMENT.md) · [`SETUP.md`](SETUP.md) | The dev stack, the test harnesses, optional dependencies and the `cc` shell function |
| [`INSTALLED_DATABASE.md`](INSTALLED_DATABASE.md) · [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md) · [`OFFLINE_DEV.md`](OFFLINE_DEV.md) · [`development/`](development/) | Databases, the fallback, working offline, PostgreSQL 19 and `sqlx` |
| [`TROUBLESHOOTING.md`](TROUBLESHOOTING.md) | Checkout and stack problems (user problems: the user guide) |
| [`SESSION_CONTAINERS.md`](SESSION_CONTAINERS.md) · [`GITHUB_WORKFLOWS.md`](GITHUB_WORKFLOWS.md) | Containerised dev sessions; what CI runs |
| [`frontend/`](frontend/) · [`tui/`](tui/) | Frontend conventions, components, routes, style guide; terminal UI style and layout |

**Releasing**

| Doc | Read it for |
|---|---|
| [`RELEASING.md`](RELEASING.md) | The hub: the alpha route, what is prepared, cargo-dist and Homebrew |
| [`RELEASE_SOURCE_GATES.md`](RELEASE_SOURCE_GATES.md) · [`RELEASE_CONTENT_REVIEW.md`](RELEASE_CONTENT_REVIEW.md) | Source gates; secrets, notices and private words |
| [`RELEASE_ACCEPTANCE.md`](RELEASE_ACCEPTANCE.md) · [`RELEASE_ACCEPTANCE_MATRIX.md`](RELEASE_ACCEPTANCE_MATRIX.md) · [`RELIABILITY_RUNNER_IMAGE.md`](RELIABILITY_RUNNER_IMAGE.md) | The acceptance journeys, the matrix (pinned by `scripts/release/`), the reliability runner |
| [`RELEASE_PUBLICATION.md`](RELEASE_PUBLICATION.md) · [`RECOVERY.md`](RECOVERY.md) · [`PILOT_FEEDBACK.md`](PILOT_FEEDBACK.md) | Publication tooling; backup, restore and upgrade; the alpha feedback kit |
