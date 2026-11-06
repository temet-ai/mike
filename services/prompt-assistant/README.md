# prompt-assistant

A small Node/TypeScript service that runs either Claude Agent SDK or Codex App
Server to help the operator reword the Claude Code system prompt mike captures
and rewrites. It is section 3 of
`artifacts/2026-09-22/prompt-assistant/spec.md` — the backend for the
"Prompt assistant" panel on `/filter/workbench` and the
`/settings/assistants` page (sections 4.4/4.5, built separately).

## What it is

- Binds `127.0.0.1:${PROMPT_ASSISTANT_PORT:-9985}`.
- Talks to mike's inspect API (`INSPECT_URL`, default `http://127.0.0.1:9989`)
  for everything: settings, filter rules, dry-runs, the prompt-class
  catalogue, and the captured request itself. It never touches mike's
  database directly.
- Runs `@anthropic-ai/claude-agent-sdk`'s `query()` **sandboxed**: `tools: []`
  disables every built-in tool (Bash, Read, Write, Edit, WebFetch, …),
  `disallowedTools` names them again as a second guard, and a deny-by-default
  `canUseTool` (`src/agent/sandbox.ts`) is a third — only the in-process
  `mcp__prompt_assistant__*` tools this turn registered are ever allowed.
  `settingSources: []` + `strictMcpConfig: true` + a neutral `cwd` (the
  service's own state dir) mean the agent never loads a project's/user's
  CLAUDE.md, output style, plugins, or `.mcp.json` — it cannot see, let alone
  call, `mcp__mike__*` or any other server's tools. The SDK subprocess's `env`
  is built from an explicit allowlist (`src/agent/env.ts`), never
  `{ ...process.env }`: `PATH`/`HOME`/`USER`/`LANG`/`TMPDIR`, `NODE_*`,
  auth-relevant `ANTHROPIC_*` vars, and `ANTHROPIC_BASE_URL` only when
  `prompt_assistant.capture` is on. This service itself runs *inside* an
  operator's own Claude Code session (via `scripts/start-proxy.sh`), so
  without this allowlist that session's `CLAUDE_EFFORT`,
  `CLAUDE_CODE_CHILD_SESSION`, and remote-control bridge/messaging vars would
  otherwise leak into the agent it spawns — `CLAUDE_EFFORT` in particular used
  to silently override the `effort` setting.
- When `assistant.provider` is `codex`, starts `codex app-server` and reuses its
  existing ChatGPT login without reading or copying credentials. The thread is
  ephemeral, uses an isolated temporary cwd, disables shell, web, apps, plugins,
  and subagents, and receives the same gated Mike tools as client-executed
  dynamic tools.
- Reads its configuration (model, effort, instructions, budget, which tools
  are exposed, …) from mike's settings API on **every message** — no restart
  needed to pick up an operator change. Missing `prompt_assistant.*` keys
  (e.g. before the settings-registry half of this change ships) fall back to
  the defaults in `src/settings.ts`, which mirror the spec's table exactly.
- Conversations are small JSON files, one per conversation, plus a
  budget ledger, both under `~/.local/state/mike/prompt-assistant/`
  (override with `PROMPT_ASSISTANT_STATE_DIR`, which is how the tests keep
  the real state directory untouched).

## Running it

```sh
pnpm install
pnpm run build && pnpm run start     # production: dist/server.js
# or, for local iteration:
pnpm run dev                          # tsx, no build step
```

It expects the selected provider's CLI login on the machine: `claude` for the
Claude Code mode or `codex` for the ChatGPT-backed Codex mode. No Codex token or
API key handling lives in this service.

## Env vars

| Var | Default | What |
|---|---|---|
| `PROMPT_ASSISTANT_PORT` | `9985` | HTTP bind port |
| `PROMPT_ASSISTANT_HOST` | `127.0.0.1` | HTTP bind host |
| `INSPECT_URL` | `http://127.0.0.1:9989` | mike's inspect REST API |
| `PROMPT_ASSISTANT_CAPTURE_BASE_URL` | `http://localhost:9988` | `ANTHROPIC_BASE_URL` the SDK subprocess gets when `prompt_assistant.capture` is on, so its own traffic is captured by mike like any other session |
| `PROMPT_ASSISTANT_STATE_DIR` | `~/.local/state/mike/prompt-assistant` | conversations + budget ledger. Override for tests/dev so they never touch the real state |
| `PROMPT_ASSISTANT_SWEEP_INTERVAL_MS` | `3600000` (1h) | how often the `keep_days` sweep runs |
| `PROMPT_ASSISTANT_MODEL_OVERRIDE` | unset | forces the model regardless of `prompt_assistant.model` in mike's settings — not part of the spec, added for cheap local smoke-testing (e.g. `claude-haiku-4-5`) |
| `PROMPT_ASSISTANT_CLAUDE_HOME` | `~/.claude` | root the `read_memory` tool reads `projects/<slug>/memory/*.md` under |

## The API

All routes are under `/api/assistant` so the frontend's vite dev proxy can
route them to this service (see the wiring note below).

- `GET /api/assistant/health` → `{ ok, enabled, provider, model, effort, may_apply, spent_today_usd, daily_budget_usd }`
- `GET /api/assistant/integrations/codex` → Codex login, plan, rate-limit windows, and token usage
- `POST /api/assistant/conversations` `{ request_id }` → `{ id }`
- `GET /api/assistant/conversations/:id` → `{ id, request_id, messages: [...] }`
- `POST /api/assistant/conversations/:id/messages` `{ text, quote? }` →
  `text/event-stream` with these event names/payloads (exactly per spec):
  - `event: text` `{ delta }`
  - `event: status` `{ phase }` — emitted by the tool handlers themselves as
    they run (`"reading prompt"`, `"reading edits"`, `"dry-run"`, `"reading
    project instructions"`, `"reading prompt versions"`)
  - `event: proposal` `{ id, title, edit_id, action, match, rule_text, before, after, reach }` —
    `rule_text` is the raw text to save on the filter rule (`null` for `drop`);
    `before`/`after` are display-only (`after` is match-concatenated for `append`,
    which would duplicate the anchor if saved literally — save `rule_text` instead)
  - `event: applied` `{ proposal_id, rule_id }` — only when
    `prompt_assistant.may_apply` is `apply`
  - `event: error` `{ message }`
  - `event: done` `{ cost_usd }`

A message is refused (a single `error` event, no `done`, the provider never
invoked) when the `prompt_assistant` feature toggle is off. Anthropic messages
are also refused when today's recorded spend meets
`prompt_assistant.daily_budget_usd`; ChatGPT-backed Codex messages use account
rate limits instead of the dollar ledger.

## The agent's tools

Registered as an in-process MCP server (`createSdkMcpServer` +
`tool()`/zod schemas), server name `prompt_assistant`. Each tool's wire name
is `mcp__prompt_assistant__<name>`, and that full list is what gets passed as
`allowedTools` — the run has no `canUseTool` callback and no TTY, so a tool
call for anything not on that list is auto-denied rather than prompted for.

| Tool | Gated by | What |
|---|---|---|
| `get_prompt({request_id})` | `read_prompt` | System blocks + session message of a request, from `GET /api/requests/:id` |
| `list_edits()` | `read_edits` | The filter rules (title, match, action, text, enabled, fires), from `GET /api/filter/rules` |
| `dry_run({request_id, rules})` | `read_prompt` **and** `read_edits` | The engine's result for a candidate rule set, from `POST /api/filter/rules/dry-run`. The spec names five `read_*` settings for six tools and doesn't spell out which one gates `dry_run`; since it needs both the prompt and the rule set, this service gates it on both being on. |
| `read_project_instructions({request_id})` | `read_project_instructions` | `CLAUDE.md` + `.claude/rules/*.md` under the request's project path, read-only, each file capped at 64 KiB (with a `truncated` flag, never a silent slice — same shape the repo's inspection-fidelity convention uses elsewhere) |
| `read_memory({request_id})` | `read_memory` (default **off**) | The project's Claude Code memory: `~/.claude/projects/<slug>/memory/*.md`, `MEMORY.md` first then the rest alphabetically, read-only, same 64 KiB-per-file cap. `<slug>` is the request's project path with every `/` replaced by `-` — Claude Code's own on-disk convention, verified against the directories that actually exist under `~/.claude/projects/` on this machine (e.g. `/mnt/data/projects/temet/temet-proxy` → `-mnt-data-projects-temet-temet-proxy`). A project with no `memory/` directory yet (never opened by Claude Code) returns `{files: []}`, not an error. |
| `list_prompt_versions({model?})` | `read_versions` | The prompt-class catalogue, from `GET /api/prompts/classes` |
| `propose_edit({title, edit_id?, action, match, text, reach})` | always on | Records a proposal and streams it as `event: proposal`. See "Proposals" below. |

## Proposals

`propose_edit`'s schema has no `before` field — the model cannot assert what
the "before" text is. Instead, the handler independently fetches the
request via the inspect API and computes `before` as the literal `match`
substring **only when it is actually present** in the request's captured
system blocks + session message; otherwise `before` is `''`. `after` is
computed from `action`, for DISPLAY only:

- `replace` → `text`
- `append` → `match + text`
- `drop` → `''`

The `rule_text` field carried on the same event is separate and is what a
caller should actually save on the filter rule: `text` for `replace`/`append`,
`null` for `drop`. It is never the match-concatenated `after` — saving `after`
literally for an `append` proposal would duplicate the anchor text (this was
G3: preview/apply used `after` and produced `# Session-specific guidance#
Session-specific guidance PROPOSAL_TAIL`-style duplicates).

When `prompt_assistant.may_apply` is `apply`, the handler itself (not the
model) calls mike's filter-rule API — `POST /api/filter/rules` for a new
edit, `PATCH /api/filter/rules/:id` when `edit_id` is given — immediately
after recording the proposal, and emits `event: applied`. With `suggest`
(the default) it only ever streams the proposal; nothing is written.

## Budget

For Anthropic turns, cost comes from the Agent SDK result message's `total_cost_usd` — an
estimate, not a billing statement, per the SDK's own docs. Spend is added to
`~/.local/state/mike/prompt-assistant/budget-ledger.json`, keyed by the
service process's **local** calendar day (whatever `TZ` the host runs in —
the operator's own day, not UTC), immediately after each turn — including a
turn that ends in an error, or whose underlying SDK stream throws partway
through, since the API call still cost money either way; `runTurn` catches
its own stream errors internally and always returns whatever partial
text/cost it saw rather than throwing, precisely so the caller can still
record that spend. `handleHealth`'s `spent_today_usd` is this same
local-day figure.

A new message is refused **before** the SDK is invoked once today's recorded
spend has reached `prompt_assistant.daily_budget_usd`. Because that check
only runs once, up front, two concurrent turns can each pass it and then
together overshoot the budget; `runTurn` also re-checks periodically (at most
once every 2s of wall-clock time) while a turn is streaming, and aborts its
own `AbortController` — ending the turn early, with an `error` event — the
moment the budget is gone. The same `AbortController` is tied to the HTTP
response's `close` event, so a client disconnecting (e.g. a "Stop" action)
also ends the SDK query immediately rather than letting it run to completion
unobserved.

## Conversations and the keep_days sweep

Each conversation is `~/.local/state/mike/prompt-assistant/conversations/<uuid>.json`.
An hourly timer (`PROMPT_ASSISTANT_SWEEP_INTERVAL_MS`) deletes any
conversation whose most recent message (or creation time, if it has none) is
older than `prompt_assistant.keep_days`.

A conversation id in a URL (`GET`/`POST .../conversations/:id[/messages]`) is
validated against a plain UUID pattern **before** it is ever used to build a
filesystem path — `http/util.ts`'s `isValidConversationId` — and the route
returns `400` for anything else. This matters because the route pattern only
excludes a literal `/`, so a segment like `..%2Fbudget-ledger` still matches
it and only becomes `../budget-ledger` after `decodeURIComponent`; without
the UUID check that traversed out of `conversations/`.

## Wiring into the rest of the stack

- `frontend/vite.config.ts` proxies `/api/assistant` → this service,
  registered **before** the general `/api` → inspect-API proxy entry (object
  key order matters for `http-proxy-middleware`/vite's proxy matching).
- `scripts/start-proxy.sh` builds and starts this service alongside the SPA
  (log at `.deploy/prompt-assistant.log`), and `scripts/stop-proxy.sh` stops
  it. A running instance is left alone only when it is both up **and**
  up to date: the script always runs an (incremental, cheap) build, hashes
  every `dist/*.js` file, and compares that against a stamp
  (`.deploy/prompt-assistant.dist-sha`) written the last time it (re)started
  the service — a changed build output restarts it even without `--force`,
  mirroring `bin_sha`/`bins_differ`'s content-hash approach for the Rust
  proxy/proxy-forward. The service is started with several parent-session
  vars explicitly unset (`env -u CLAUDE_EFFORT -u CLAUDE_CODE_CHILD_SESSION`,
  …) — belt-and-braces alongside `src/agent/env.ts`'s own allowlist, so the
  *service process itself* never carries them into `dist/server.js`, which
  then only spreads its own allowlisted env into the SDK subprocess.

## Tests

`pnpm run test` (vitest). Everything is mocked: `queryFn` is injected (see
`AppDeps.queryFn`) so no test ever spawns the real Agent SDK subprocess, and
every inspect-API call goes through `InspectClient`'s injectable `fetchImpl`
(`test/fakes.ts`'s `stubFetch`) rather than a live proxy. No sleeps, no
network. Coverage: settings resolution + defaults, tool gating by every
`read_*` setting (including `read_memory`), SSE event framing, budget refusal
(both feature-toggle-off and budget-exhausted paths) plus local-day
accounting and mid-turn abort-on-exhaustion, the `keep_days` sweep,
`propose_edit` in both `suggest` and `apply` mode (including the
before-text-not-found case and `rule_text` for replace/append/drop),
`read_memory`'s file ordering/truncation/no-memory-yet/path-containment
behavior against a throwaway tmp directory — never a real `~/.claude` — the
sandbox options passed to `query()` (`settingSources`, `strictMcpConfig`,
`canUseTool`, `disallowedTools`, `cwd`), the env allowlist (no `CLAUDE_*`
leakage even when set), and conversation-id UUID validation at the route
layer (`test/app.test.ts`, including the `..%2F` case).

## e2e (opt-in, costs real money)

`scripts/e2e.ts` (`pnpm run e2e`) is the real thing: health → create a
conversation → send one real message → assert on the SSE event types, not
mocks. It is a no-op — prints one line and exits 0 — unless
`PROMPT_ASSISTANT_E2E=1` is set, so a normal `pnpm run test`/CI run never
spends anything or needs a running service.

```sh
# in one terminal — a throwaway instance, NOT the real :9985 service:
PROMPT_ASSISTANT_PORT=9975 \
PROMPT_ASSISTANT_STATE_DIR=/tmp/prompt-assistant-e2e-state \
PROMPT_ASSISTANT_MODEL_OVERRIDE=claude-haiku-4-5 \
  pnpm run start

# in another terminal:
PROMPT_ASSISTANT_E2E=1 \
PROMPT_ASSISTANT_E2E_URL=http://127.0.0.1:9975 \
PROMPT_ASSISTANT_E2E_REQUEST_ID=421514 \
  pnpm run e2e
```

Env vars: `PROMPT_ASSISTANT_E2E_URL` (default `http://127.0.0.1:9985`),
`PROMPT_ASSISTANT_E2E_REQUEST_ID` (default `421514`),
`PROMPT_ASSISTANT_E2E_QUESTION` (default the same one-sentence Harness-section
question used during development). It asserts `health.ok`/`health.enabled`,
that the conversation round-trips, that the message stream carries at least
one `text` event, exactly one `done` event with a numeric `cost_usd`, zero
`error` events, and that the conversation is persisted with a `user` then
`assistant` message afterward — then prints every SSE frame it saw plus the
final cost. Never point `PROMPT_ASSISTANT_E2E_URL` at a real operator's
`:9985` instance — it sends one real, billed message on whatever conversation
it creates.
