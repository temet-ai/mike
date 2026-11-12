---
title: Observing Codex sessions mike did not launch
status: active
created: 2026-11-11
updated: 2026-11-11
summary: Install Codex hooks and OpenTelemetry export so mike records sessions, turns, tools and subagents of ordinary Codex sessions, observe-only.
session_id:
  - a038f124-199b-4695-80de-ee5aacd48bc2
---

# Observing Codex sessions mike did not launch

Validated against `codex-cli 0.167.1` (issue temet-ai/mike#23).

## Install

```bash
scripts/codex-hooks/install --codex-home ~/.codex          # your own sessions
scripts/codex-hooks/install --codex-home "$(mktemp -d)"    # a trial home
scripts/codex-hooks/install --codex-home ~/.codex --print  # show, write nothing
```

`--codex-home` is required, so a test or a trial never writes into `~/.codex` by
accident. The installer adds one observe-only handler per hook event beside any handlers
already in `hooks.json`, and an `[otel]` block exporting logs and traces as OTLP/JSON to
mike (it prints the block instead when `config.toml` already has `[otel]`). Codex does not
run a new hook until you trust it (`/hooks` in the TUI, recorded as `hooks.state.<key>.trusted_hash`
in `config.toml`); an edited hook reads `modified` and stops running until trusted again.

## What mike receives

| Route | Sender | What it stores |
|---|---|---|
| `POST /api/codex/observe/hooks` | `mike-codex-hook`, from every hook event | The payload bytes in `runtime.events`; thread, turn, tool and model-request rows |
| `POST /api/codex/observe/otel/v1/{logs,traces}` | Codex's `otlp-http` exporter, `protocol = "json"` | The export bytes in `runtime.events`; tool results, request attempts, spawns and hook command runs |
| `GET /api/codex/observe/sessions` | the operator | Observed threads with their tools, sources and the coverage gaps below |

Every row says which source saw it (`hook`, `telemetry`, or both). A tool call seen by both
is one row keyed by Codex's call id. A hook delivery carries `x-mike-delivery-id`, so a
spooled delivery sent twice is recognised and changes nothing; an OTLP export is keyed by
its own bytes. Telemetry output is a snippet and is never treated as captured output.
Model-request hooks and telemetry share no request id, so the two are kept per source and
never added together.

## Control

None. `mike-codex-hook` prints nothing and exits 0, so it never blocks, allows or changes
what Codex does; every answer of the routes says `"control": "none"`. A Codex thread mike
launched itself appears with `managed_by_mike: true`.

## Failures and deadlines

When mike is down the hook spools the payload under
`${XDG_STATE_HOME:-~/.local/state}/mike/codex-hooks` and sends it on a later call. The
installer marks the handlers `async` with a 5 s timeout (3 s for `SessionEnd` and
`Interrupt`, which Codex clamps to 3 s). A handler that runs past its timeout is recorded
from the `codex.hooks.command` span with outcome `timeout`.

## Gaps

`GET /api/codex/observe/sessions` lists them as `coverage_gaps`: hosted tools
(web search, image generation) send no hooks; `write_stdin` polls of a long command are
seen only in telemetry and are linked to their command by process session id; a nested
code-mode call carries no id of its outer `exec` call; hook payloads carry no timestamp;
`PermissionRequest`, `PreCompact`, `PostCompact` and `Interrupt` were not exercised in the
recordings.

## Fixtures

`fixtures-from-recording` turns a recorded run (hook stdins and OTLP exports from a temp
`CODEX_HOME`) into the scrubbed fixtures under `crates/mike/src/runtime/codex_observe/fixtures/`.
