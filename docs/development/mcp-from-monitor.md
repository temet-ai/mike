---
title: Using mike MCP from a Background Monitor / cron context
status: active
created: 2026-06-13
updated: 2026-11-06
summary: Explains why shell-based monitors cannot call MCP tools and documents the sanctioned pattern for periodic proxy inspection.
---

# Using mike MCP from a Background Monitor / cron context

## The problem

`Monitor`, `tail -f`, and cron shell scripts run in a bare shell process. The
`mcp__mike__*` tools exist only inside an **agent turn** — they are
JSON-RPC calls over the MCP stdio transport that the `mike-mcp` binary serves
to Claude Code. A shell script has no access to that transport, so any attempt
to call `mcp__mike__get_session_overview` or similar from within a
`Monitor` command will silently do nothing.

## The sanctioned pattern

The shell's only job is to **wake the agent**. The agent, running in its own turn, then
calls the MCP tools. A monitor session polls on its own cadence: either a Claude Code
scheduled task or the repo's `/loop` skill re-invokes the agent with a prompt such as
"call `list_tool_errors` with `limit=10` and report any `is_error=true` rows", and the
agent calls the tools and reports in that turn. The same holds when `Monitor` is the
trigger: the Monitor command emits a line, Claude Code delivers it as a notification,
and the agent responds by calling MCP tools.

## MCP tools suited to a watch loop

| Goal | Tool |
|---|---|
| New activity in a session | `mcp__mike__get_session_overview` |
| Tool errors since last check | `mcp__mike__list_tool_errors` |
| New diagnostic findings | `mcp__mike__diagnostic_findings` |
| Requests in a session | `mcp__mike__get_session` |
| Full body of a specific request | `mcp__mike__get_request_body` |
| Find text across a session's bodies (or the whole corpus) | `mcp__mike__find_text` (pass `session_id`/`request_id` to scope; omit to search all bodies — defaults to the last 7 days, override with `date_from`/`date_to`) |

All of these tools accept an optional `select` (array of RFC 6901 pointers with
a `*` array/object wildcard) projecting the result to just the head/status
fields a tight poll loop needs — omit for the full result, leaves come back
whole. Caller-driven projection, not truncation; see `docs/MCP_BRIDGE.md`.

All of these are in the default `core` tool profile (`mike-mcp` launched with
no flag), so no `--full` relaunch is needed for a watch loop. They work
unchanged in the woken agent turn — no special setup is required beyond
having `mike-mcp` running and the proxy stack healthy, pointed at the inspect
bind via `MIKE_INSPECT_URL` (default `http://127.0.0.1:9989`).

## What NOT to do

Do not embed `curl http://127.0.0.1:9989/api/...` calls inside a Monitor
script as a workaround. REST calls from shell work, but they bypass the MCP
layer, produce unstructured output the woken agent must parse, and are
harder to maintain. Use the sanctioned pattern above and let the agent call
the MCP tools in its own turn.

## Future work

A purpose-built `watch_session` long-poll MCP tool (tracked in [issue #134](https://github.com/temet-ai/mike-private/issues/134), gap #4) would let a single agent turn subscribe to session events
without re-invoking the agent on each tick. Until that tool exists, the
re-invocation pattern above is the correct approach.
