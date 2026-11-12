---
title: Codex Responses capture
status: active
created: 2026-11-11
updated: 2026-11-12
summary: Bounded Codex observation, optional diagnostic wire evidence, and the verified authentication and transport boundary.
session_id:
  - 83f69e4d-2806-46e6-8748-85f6ad4cbb16
---

# Codex Responses capture

Verified against **codex-cli 0.167.1** (`codex --version`, 2026-11-11). Re-verify on every Codex
upgrade: the facts below are observed behaviour, not documented contract.

Managed app-server client development is covered in
[Developing the Codex client](CODEX_DEVELOPMENT.md). Its crate boundary is separate
from this wire-capture path.

## How to start a captured Codex session

Source `scripts/shell/cdx.zsh` and run `cx` where you would run `codex`, the way `cc` starts
Claude Code. `cx` adds `--dangerously-bypass-approvals-and-sandbox`; `cdx` is the same launcher
without it. Both add `-c openai_base_url="http://127.0.0.1:9988/codex"`. When
`GET :9989/api/health` does not answer, they print `cx: mike not reachable … starting Codex
UNCAPTURED` on stderr and start plain `codex`. The file removes an older `alias cx=…` when it
is sourced; `scripts/cx`, the cargo wrapper, is run by its path and does not clash.

## What mike does with it

The proxy bind (`:9988`) serves `/codex/{*rest}` (`crates/mike/src/responses_capture/`):

1. The request body is streamed without an unbounded whole-body buffer. The upstream is chosen from the credentials: a
   `chatgpt-account-id` header means a ChatGPT login and goes to
   `https://chatgpt.com/backend-api/codex`; any other request goes to `https://api.openai.com/v1`.
   Override with `MIKE_RESPONSES_CHATGPT_UPSTREAM` / `MIKE_RESPONSES_OPENAI_UPSTREAM`.
2. The forwarded body is the inbound body unchanged (no Codex mutation stage exists); headers go
   through the same `filter_request_headers` as Claude traffic (hop-by-hop removed, `Host`
   rewritten, `accept-encoding: identity`).
3. The response streams to the client unchanged. `mike-codex` incrementally extracts response
   identity, observed model, status and nullable usage into common operational facts. A terminal
   SSE event is recognized before transport EOF: a client closing after that event is not an
   aborted response. A fully relayed Content-Length body is also complete without another poll.
   Per-frame parsing is bounded at 1 MiB; oversized or invalid frames are reported as observation
   gaps. Forwarded bytes are unaffected.
4. A WebSocket upgrade on `/codex/responses` is answered `426 Upgrade Required` and recorded as
   `transport = websocket_refused`. Codex 0.167.1 then falls back to HTTP POST with SSE for the
   rest of the session (observed on both auth routes).
5. An existing thread binding links wire facts to a Mike session only when it is
   unambiguous. The optional `x-mike-codex-home` header can disambiguate registered
   local namespaces; an invalid explicit home never falls back to another namespace.
   Unknown or ambiguous provider IDs remain available for inspection but are not
   guessed to be Mike identities. Ordinary hook and
   metadata-telemetry observation supplies the session lifecycle and usage independently.

## Storage and observation policy

Routine managed/hook/telemetry observation stores session state, usage and terminal item
metadata without raw payloads, tool arguments, output bodies or duplicate context transcripts.
The shared roster, fleet seat and session views read this common layer. The launcher installs
compact lifecycle/mail hooks and Mike MCP setup; Codex's own hook trust review still applies.
See [the hook setup](../scripts/codex-hooks/README.md) for receipt and idle wakeup semantics.

`codex_diagnostic_capture` defaults off and can be changed live in `/settings/workers` or the
existing feature API. When enabled, original request and response evidence is admitted whole
within 4 MiB per exchange, 32 MiB per session and 256 MiB installation limits. Unchanged
forwarded request bytes reference the original instead of being stored again. Output items
are not copied out of the response body. At most 16 exchanges can hold observation buffers or
await a database write; saturation records a metric/logged gap and does not block forwarding.

Evidence states are `not_collected`, `retained`, `budget_refused`, `incomplete` and `expired`.
An incomplete or over-budget body is not retained as a misleading prefix. Raw evidence expires
after 24 hours; diagnostic metadata has a separate seven-day expiry. Common facts have finite
row/byte budgets and a 90-day history window; retry receipts expire separately. Expiry is
batched by the retention worker. Admission remains bounded even when sweeping is disabled.
Historical Codex raw/projection records are swept under the migration's declared policy;
legacy Claude retention is unchanged.

These are logical admitted-data budgets, not a promise that PostgreSQL files, indexes, WAL or
backups occupy exactly those bytes. Deletion normally makes database pages reusable. Missing
usage stays unknown, and runtime completion is separate from fleet acceptance or mail receipt.

### Correlation, with its evidence

`correlation` (JSONB) says where each id came from or that it is unknown. Sources, in order: the
`session-id` and `thread-id` headers, then the `x-codex-turn-metadata` header (`session_id`,
`thread_id`, `turn_id`, `request_kind`, `workspaces`), then `<cwd>` in the body's
`<environment_context>` in retained diagnostic evidence. Routine streamed requests use
headers for correlation. A prewarm request has an empty `turn_id`, recorded as unknown.
A provider UUID alone does not prove Mike session identity. Nothing is read from
`experimentalRawEvents`.

## Observed wire behaviour (codex-cli 0.167.1, `codex exec`, local stub upstream)

| Route | Requests seen | Body |
|---|---|---|
| API key (`CODEX_API_KEY`) + `openai_base_url` | `GET /v1/responses` WebSocket upgrade (prewarm, then turn), then on 426 `POST /v1/responses` | plain JSON |
| ChatGPT login + `openai_base_url` | `GET /v1/models?client_version=…` (×4), WebSocket upgrade, then on 426 `POST /v1/responses` | `content-encoding: zstd` |
| `OPENAI_BASE_URL` env var only | not honoured: Codex called `https://api.openai.com/v1/responses` | — |

`openai_base_url` therefore redirects both auth routes' model calls, and the ChatGPT route's
model list, in this version. It does not redirect login, token refresh or telemetry calls.

## Uncaptured paths (flagged, not hidden)

- **WebSocket Responses transport.** Refused rather than relayed; frames over a WebSocket are
  never captured. If a future Codex stops falling back to HTTP, its turns fail visibly through
  `cdx` instead of passing uncaptured.
- **Codex started without `cdx`.** Goes straight to OpenAI. Through the forward sidecar
  (`HTTPS_PROXY=:9987`) its CONNECTs to `api.openai.com` and `chatgpt.com` are blind tunnels; the
  sidecar now logs them at `warn` naming `cdx` (`proxy-forward/src/connect.rs`). The sidecar's
  MITM, cert resolver and upgrade bridge stay Anthropic-only (`intercept_host`), and any
  WebSocket upgrade through the sidecar remains a direct, uncaptured bridge (`route/upgrade.rs`).
- **Non-model calls** (auth refresh, `chatgpt.com` analytics, plugin and MCP traffic) do not use
  `openai_base_url` and are not captured.
- **Compressed bodies.** Opt-in zstd request evidence is stored byte-exact but not decoded, so request `model`
  is NULL for them; the request's own model is in `x-codex-turn-metadata`.

## Combinations not exercised on this machine

- API key against the real OpenAI upstream: no `OPENAI_API_KEY` here; the API-key route was run
  only against a local stub.
- ChatGPT login against the real `chatgpt.com` through mike: not run by the tests (they use a
  local stub); the redirect itself was observed live with a stub upstream.
- Retries after a mid-stream upstream failure, response compression from the real upstream,
  `codex` interactive TUI and `app-server` turns: the routing code is shared, but only
  `codex exec` was run.
- Correlation of a WebSocket continuation (`previous_response_id` over one socket): impossible
  while WebSockets are refused.
