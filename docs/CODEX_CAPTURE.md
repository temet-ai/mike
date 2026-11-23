---
title: Codex Responses capture
status: active
created: 2026-11-11
updated: 2026-11-23
summary: Complete Codex proxy capture, shared body references, and the authentication and transport boundary.
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

The proxy bind (`:9988`) serves `/codex/{*rest}` (`crates/mike-features/src/proxy/responses_capture/`):

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
   gaps. Complete capture and forwarded bytes are unaffected.
4. A WebSocket upgrade on `/codex/responses` is answered `426 Upgrade Required` and recorded as
   `transport = websocket_refused`. Codex 0.167.1 then falls back to HTTP POST with SSE for the
   rest of the session (observed on both auth routes).
5. Spawned sessions use `/codex/launch/<Mike-session-UUID>` as their provider base URL.
   The proxy validates that open Codex launch, binds the native thread to its reserved
   identity, and keeps child threads distinct. Ordinary `/codex` traffic can register
   its native thread directly. Hooks are not required for capture or identity.

## Capture and storage

Every exchange retains its complete request and response content, including unknown
fields and events. Parsing supplies model, usage and status projections; a parser gap
cannot remove bytes from capture. Interrupted streams retain the bytes received and
are marked incomplete. Original and forwarded request documents share a reference
while forwarding is unchanged. Future injection can record a separate sent document
without copying unchanged content.

`mike-codex::capture` divides bodies into chunks and compares them with chunks from a
known predecessor. The generic store carries matching chunk IDs forward and inserts
new bytes using returned IDs. Each document has its own ordered references, so
identical requests remain separate occurrences and reconstruction needs no recursive
chain. No global hash lookup or content-hash index is involved.

The stream uses file staging instead of an unbounded memory buffer. Chunk boundaries
bound working memory; they do not truncate payloads. Failed commits retain staging
for recovery. The existing session view lists exchanges and reads a body only when
opened, without adding another persistent browser copy.

This capture path introduces no automatic content expiry, per-body limit or storage
quota. Sessions with captures are excluded from automatic session expiry. Retention
and deletion policy are separate operator decisions. Existing bounded metadata
projections and legacy diagnostic retention do not govern these documents.

`codex_diagnostic_capture` is not required for full proxy capture. The old diagnostic
record remains a metadata projection; it does not add a second copy of these bodies.
Missing usage stays unknown, and a completed response is separate from mail receipt
or fleet acceptance.

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
- **Codex started without a proxy launcher or provider URL.** Goes straight to OpenAI. Through the forward sidecar
  (`HTTPS_PROXY=:9987`) its CONNECTs to `api.openai.com` and `chatgpt.com` are blind tunnels; the
  sidecar now logs them at `warn` naming `cdx` (`proxy-forward/src/connect.rs`). The sidecar's
  MITM, cert resolver and upgrade bridge stay Anthropic-only (`intercept_host`), and any
  WebSocket upgrade through the sidecar remains a direct, uncaptured bridge (`route/upgrade.rs`).
- **Non-model calls** (auth refresh, `chatgpt.com` analytics, plugin and MCP traffic) do not use
  `openai_base_url` and are not captured.
- **Compressed bodies.** Complete zstd payloads are decoded for storage, with the original
  headers and stored encoding recorded separately. The upstream receives the original bytes.
  Invalid or unsupported encoding falls back to retaining the encoded body and a decode error.

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
