---
title: Security and trust boundary
status: active
created: 2026-09-17
updated: 2026-11-06
summary: What each mike bind exposes, what the Host/Origin guard does and does not stop, where captured data lives, what leaves the machine when a feature is enabled, and how to run with nothing enabled.
---

# Security and trust boundary

mike captures the full text of everything you say to Claude Code and everything Claude Code's
model says back, including system prompts, file contents read by tools, and tool results. That
data is as sensitive as the repositories you work in.

**The short version: mike is designed for one person on one machine, bound to loopback, with no
authentication anywhere. Any process on the machine can read all of it.** There is no
supported way to expose it to a LAN or the internet.

To report a vulnerability, follow [`../SECURITY.md`](../SECURITY.md): use GitHub's private
reporting on the repository, and put no exploit details, credentials or captured conversation
content in an ordinary issue.

## The trust boundary

mike trusts every process running on the machine it runs on. It does not authenticate anything.
The only boundary it enforces is against a *browser* being tricked into talking to it from
another site, and against DNS rebinding. Both are the Host/Origin guard described below.

## What each bind exposes

The `mike` binary opens up to three listeners, all on `127.0.0.1` by default.

| Bind | Default | What it serves | Who can reach it |
|---|---|---|---|
| Proxy | `127.0.0.1:9988` | `/v1/*` and `/v2/*`, forwarded to `upstream_url` (`https://api.anthropic.com`) | Any local process. mike holds no Anthropic credential of its own — it forwards whatever credential the caller sends, so a caller must supply its own key. What mike *does* provide for free is capture: anything sent through this bind is recorded. |
| Inspect | `127.0.0.1:9989` | The entire `/api/*` REST surface plus the `/api/ws` WebSocket | Any local process, with no credential. See below for what that surface allows. |
| SPA | `127.0.0.1:9990` | The embedded browser UI **and the same entire `/api/*` surface** | Any local process. Disabling this bind (`MIKE_SPA_ENABLED=0`) does not reduce the API's exposure, because `:9989` still serves all of it. |

The optional `proxy-forward` sidecar is a separate binary on `127.0.0.1:9987`. It is an HTTPS
MITM: it terminates `CONNECT` tunnels, mints per-host certificates from a CA it generates on
first run, and forwards the decrypted traffic. Clients trust it by loading that CA
(`NODE_EXTRA_CA_CERTS`). A CA on disk that a client trusts can sign a certificate for any host,
so treat the sidecar's CA file as a credential and do not install it into a system trust store.
The CA key is written with mode `0600`.

**Keep the sidecar on loopback.** For every host other than the one it intercepts, the sidecar
opens a plain tunnel to whatever address the client names, and it does not check who the client
is or where the tunnel goes. Bound to `0.0.0.0`, it lets another machine tunnel to
`127.0.0.1:9989` and arrive at the inspect API as a loopback caller, which defeats the
Host/Origin guard below. The systemd unit and `scripts/lib/proxy-runtime.sh` bind it to
`127.0.0.1`, and the sidecar logs a warning at start when it is bound anywhere else.

### What the inspect API allows, unauthenticated

Not just reads. Among other things, any local process can:

- read every captured request, response, system prompt and tool result, in full;
- read and write every runtime setting, replace or clear the OpenRouter and Anthropic API keys
  mike holds, and turn on any feature that rewrites traffic or sends text off the machine.
  Reading a key back is a different matter: see [Where keys live](#where-keys-live);
- create, update and delete filter rules and prompt compositions, which rewrite the requests
  mike forwards;
- `POST /api/launches` to start a Claude Code session on the machine, in a directory of its
  choosing, with the arguments it chooses.

There is no per-route permission model and no read-only mode.

## Where keys live

mike never writes an API key to the database. The settings store refuses any key whose name
contains `key`, `secret`, `token`, `password` or `credential` (names ending `_env` excepted,
because they hold the name of an environment variable, not its value), and the configuration
change log replaces the value of any such key with `{"redacted": true}`.

| Provider | Where the credential is | How it reaches mike | What the API gives back |
|---|---|---|---|
| OpenRouter | The process environment: `MIKE_STEERER_OPENROUTER_API_KEY`, or `OPENROUTER_API_KEY` as a fallback; or the per-user config file, as `steerer_openrouter_api_key` or the name of a variable in `steerer_openrouter_api_key_from_env` | Read at start. `PUT /api/settings/steerer/openrouter/key` replaces it in memory only, so it lasts until the process stops | The first twelve characters and the last three, never the key |
| Anthropic, API-key mode | The process environment: `MIKE_ASSISTANT_ANTHROPIC_API_KEY` | Read at start. `PUT /api/settings/assistant/anthropic/key` replaces it in memory only | The same shortened form. The one route that returns the whole key needs the service token described below |
| Anthropic, Claude Code login | Claude Code's own credential store | mike starts the `claude` CLI's login and logout; it never reads the stored credential | Whether the CLI is signed in, and as whom |
| Codex | The Codex CLI's own credential store | The prompt assistant starts `codex app-server` and asks it for account status, with token refresh off | Whether Codex is signed in, the plan and the usage |

**The service token.** `GET /api/settings/assistant/anthropic/credentials` hands the saved
Anthropic key to the prompt assistant, which needs the real key to start its agent. Each time
mike starts it makes a new random token, keeps it in memory, and writes it to
`service-token.<inspect port>` in its state directory (`$XDG_STATE_HOME/mike`, or
`~/.local/state/mike`) with mode `0600`. The route answers `401` unless the request carries
that token in `x-mike-service-token`. The prompt assistant reads the file again on every
request, so it keeps working across a mike restart. This narrows who can read the key from
"anything that can reach the port" to "a process that can read this user's files". It does not
protect the key from another process running as the same user.

**The client's own credential.** The `authorization` or `x-api-key` header Claude Code sends is
forwarded upstream unchanged and is never stored: see the header list under
[Where captured data lives](#where-captured-data-lives).

## What the Host/Origin guard does

`crates/mike-base/src/web_base/host_guard.rs` (with `host_guard/policy.rs`) is a middleware layer on the whole inspect router, so it
applies on both `:9989` and `:9990`. It exists to stop a web page you visit from using your
browser as a proxy into mike. It does two checks.

**Host check.** The request must carry one valid `Host` authority (HTTP/2 and HTTP/3 may use the
URI authority instead; if both are present they must match). It must be the listener's own
address (unless that is a wildcard), `localhost`, `127.0.0.1` or `[::1]`, at that listener's
port, or an entry of `inspect_allowed_hosts` (`MIKE_INSPECT_ALLOWED_HOSTS`, empty by default).
A request carrying `Host: evil.example` is rejected with 403. **This is what stops
DNS rebinding**: an attacker's domain that resolves to `127.0.0.1` still sends its own name in
`Host`.

**Origin check.** It runs only when an `Origin` header is present. `Origin: null` is rejected.
Otherwise the origin must either match `spa_origins` exactly, or have the same authority as the
`Host` header. This is what stops cross-origin `fetch` and cross-site WebSocket connections —
a WebSocket upgrade is not protected by CORS, so the guard rejects it directly.

Both checks return `403` with a JSON error body.

## What the Host/Origin guard does not do

- **It is not authentication.** It checks headers the client sends; it does not establish who
  the client is.
- **It does not stop any local process.** `curl http://127.0.0.1:9989/api/requests` sends a
  loopback `Host` and no `Origin`, and passes both checks. So does anything else on the
  machine, running as any user.
- **It does not see through `X-Forwarded-*`.** Forwarded-Host and Forwarded-Proto do not change
  the policy, so a reverse proxy in front of mike is judged by the `Host` it sends.
- **It does not protect the proxy bind.** `:9988` has no guard layer; anything that can reach
  it can push traffic through mike and have it captured.
- **It does not make a non-loopback bind safe.** Adding your machine's hostname to
  `inspect_allowed_hosts` removes the guard's objection without adding any authentication. Do
  not do this to share a mike instance.

## Where captured data lives

- **Postgres.** The capture database is the durable store. Request and response bodies are
  stored whole, as bytes. Anyone with the database URL has everything. Treat that connection
  string as a credential, and do not point mike at a shared database — several of its schema
  names (`sessions`, `messages`, `requests`, `tools`, `memory`) are generic enough to collide
  with another application's.
- **The in-memory staging pool.** Bounded by `max_requests` (default 1000) and `max_age`
  (default 24h); holds in-flight and streaming state before it lands in Postgres. Under memory
  pressure, and only when `cap_manager.rss_ceiling_bytes` is non-zero, an unmutated request can
  be forwarded without being captured at all.
- **Captured headers.** Request and response headers are stored with the value of every
  credential-bearing header replaced by `[redacted]`: `x-api-key`, `authorization`, `cookie`,
  `set-cookie`, `proxy-authorization`, any header whose name ends in `token`, and any whose name
  contains `api-key`, `apikey`, `auth-token`, `access-token`, `session-token`, `secret`,
  `password` or `credential`. Rate-limit headers such as
  `anthropic-ratelimit-tokens-remaining` are kept. The credential itself is still forwarded
  upstream verbatim, as it must be.
- **Captured bodies are not filtered.** A request or response body is stored exactly as it
  crossed the wire. If a session prints a secret, for example by reading `.env`, that text is
  in the conversation, so it is sent upstream and stored with it.
- **Log files.** Written under `log_dir` when set.
- **The per-user config file** (`~/.config/mike/config.toml`). It holds the database URL, which
  contains the database password, and an OpenRouter key if you pasted one into setup (typing the
  name of an environment variable stores only the name). Setup writes it readable by you only;
  mike refuses a configuration folder that other users can enter. A `.env` in a development
  checkout is read by the launcher scripts, not by the `mike` binary, and keeps whatever mode
  you gave it.

Retention: `retention_sweep` is on by default and periodically deletes sessions older than
`default_ttl` (365 days). `sweep_at_startup` is off, so nothing is deleted at boot.

## What leaves the machine

With default settings, exactly one thing leaves: the request mike forwards to `upstream_url`,
which is the request Claude Code sent. Nothing else is transmitted anywhere.

Each of the following sends data elsewhere, and each is off by default:

| Enable this | And this leaves the machine | To |
|---|---|---|
| `steerer`, `judge_worker`, `session_judge`, `judge_lessons`, `session_end_shadow`, `turn_guard`, `steerer.refocus.enabled`, `lesson_relations.enabled` | Conversation text — prompts, assistant turns, tool-call summaries, and stored lessons — as the body of an LLM call | `openrouter.ai`, or whatever `steerer.openrouter_base_url` names |
| `embedding.enabled` with a **remote or hosted** `embedding.url` | The text being embedded: turns, lessons, project context, recall queries | Whatever that URL names. A local embedding server on `127.0.0.1` sends nothing off the machine. |
| `mike verifier-replay --execute --out <file>` (flags `--set`, `--arm`, `--max-usd` default 15, `--model`, `--reasoning-effort`, `--prompt-file`); without `--execute` it is a dry run that sends nothing | Rebuilt verifier inputs of past turns: conversation text, tool-call lists, and the notes the session had been sent with the operator's recent replies. Per-call records go to `<out>.calls.jsonl` and reports to `--out`, never to `llm.calls` | `openrouter.ai`, or whatever `steerer.openrouter_base_url` names |
| `otel_endpoint` set | Trace spans. mike's spans carry ids, sizes and counts; no prompt or response body was found on a span attribute | Your OTel collector |

On a default install an OpenRouter key is the single gate all the LLM rows share, because every
default model is an OpenRouter model: with no key set, none of them can send anything, whatever
their toggles say. A job that you set to a `claude/` or `codex/` model does not need the key; it
sends its text through the local Claude Code or Codex login instead, so choosing such a model is
choosing to send that job's text to Anthropic or OpenAI. One path is worth knowing about because it does not
follow the `steerer` toggle — a same-request judge fires when a key is present *and* a steerer
is enrolled for the session. Setting no key keeps that shut too.

`llm.calls` in the capture database records the full outbound body of each LLM call, so it is
also the audit record of exactly what was sent.

## Running with nothing enabled

A fresh install is already in this state. To confirm or restore it:

1. **Leave the optional credentials unset.** No `MIKE_STEERER_OPENROUTER_API_KEY` or
   `OPENROUTER_API_KEY`, no `MIKE_OTEL_ENDPOINT`. Without an OpenRouter key every LLM feature
   whose model runs on OpenRouter (the default for all of them) is inert even if its toggle is on.
2. **Check the toggles.** `GET /api/settings/features` lists them with their current values;
   `/settings` in the browser shows the same thing, and `mike features watch-only` sets every
   switch to its default. The ones that rewrite traffic or call out default off:
   `reminders`, `recall_injector`, `weasel_intercept`, `reminder_on_token`, `mailbox_redelivery`,
   `steerer`, `judge_worker`, `session_judge`, `judge_lessons`, `session_end_shadow`,
   `turn_guard`, `agent_lessons`, `file_read_compression`, `shell_output_compression`,
   `proxy_tools`, `self_identity_reminder`, `repeated_notices`, `workspace_briefing`,
   `native_peer_mail`, `mod_lesson_delivery` and `context_focus_question`;
   [`FEATURES.md`](FEATURES.md) lists every switch with its default. Switches that are on by
   default work on local state only, and `semantic_context` refuses a non-loopback embedder.
3. **Author no filter rules, lazy sections or prompt compositions.** All three rewrite the
   outbound request, and none of them has a feature toggle — the only gate is whether a rule
   you created matches the project. `composition_enabled` additionally defaults off.
4. **Keep the binds on loopback.** `proxy_listen_addr`, `inspect_listen_addr` and `spa_bind`
   all default to `127.0.0.1`. Leave `inspect_allowed_hosts` empty.
5. **Don't run the sidecar** unless you want Remote Control capture, and don't add its CA to a
   system trust store.

An install that has been running for a while keeps whatever settings it already has: a stored
value always wins over the code default, and a default change never rewrites stored values. If
you upgraded into this policy rather than installing fresh, check the toggles yourself.

## See also

- [`FEATURES.md`](FEATURES.md) — every optional feature with its class and default
- [`CONFIGURATION.md`](CONFIGURATION.md) — `inspect_allowed_hosts`, `spa_origins`, bind
  addresses, and the rest of the boot configuration
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — why the binds are split the way they are
- [`FORWARD_PROXY_WEBSOCKET_UPGRADE.md`](FORWARD_PROXY_WEBSOCKET_UPGRADE.md) — the sidecar
