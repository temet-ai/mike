---
title: Connecting ordinary Codex sessions to Mike
status: active
created: 2026-11-11
updated: 2026-11-12
summary: Compact lifecycle hooks, automatic mailbox notices, explicit agent receipt, and bounded local state for ordinary Codex sessions.
---

# Connecting ordinary Codex sessions to Mike

`cdx` updates this adapter in the active `CODEX_HOME` before launching Codex.
Install it explicitly for sessions launched another way:

```bash
scripts/codex-hooks/install --codex-home ~/.codex
scripts/codex-hooks/install --codex-home "$(mktemp -d)"    # isolated trial
scripts/codex-hooks/install --codex-home ~/.codex --print  # preview only
```

The installer preserves unrelated hooks and configuration, enables the hooks
feature, and adds a core-profile Mike MCP server when `mike-mcp` is on `PATH` and
no server named `mike` is configured. It warns if the binary is missing. Existing
MCP settings and operator telemetry exporters remain unchanged. When no exporter
is configured, it adds OTLP/HTTP JSON logs to Mike's
`/api/codex/observe/otel/v1/logs`, with the source label and canonical Codex home in
headers. Prompt, assistant-response and guardian text logging are disabled, and
logged tool-result text is capped at zero bytes. No trace exporter or raw
request/response capture is enabled. Existing settings that explicitly enable
content logging are preserved; in that case the metadata exporter is not added.

These logs provide observed model and token usage for ordinary sessions. Mike
accepts at most 512 KiB per export, discards raw log bodies, and stores normalized
metadata under the verified runtime identity. The
[official telemetry configuration](https://learn.chatgpt.com/docs/config-file/config-advanced)
describes the source events and logging controls.

Codex requires you to review and trust new or changed hooks through `/hooks`.
The installer never writes trust hashes or changes sandbox/approval settings.
Command/output contracts were checked against installed Codex 0.167.2 and the
[official hooks documentation](https://learn.chatgpt.com/docs/hooks); automated
script tests use fake Codex/curl, not a model session.

## Lifecycle and mail

The synchronous handlers cover `SessionStart`, `SessionEnd`, `UserPromptSubmit`,
`PostToolUse`, `Stop`, `SubagentStart`, and `SubagentStop`. Each posts one compact
metadata fact to `/api/codex/observe/hooks`, advertising
`x-mike-codex-control: hook_boundary`. Mike owns session registration, heartbeat,
identity mapping and end handling. The response supplies the canonical Mike
session UUID and at most 20 unread message UUIDs.

Only `SessionStart`, `UserPromptSubmit`, `PostToolUse` and a bounded `Stop`
continuation can emit a notice. That notice contains fixed adapter instructions
to call `get_session_messages`, treat the returned bodies as peer data, and call
`ack_session_message` after recording or acting on each message. It contains no
mail body, sender-controlled instruction, roster briefing or tool output.
Subagent events never issue notices for the parent's mailbox. Notifications are
deduplicated by pending IDs and turn; a Stop-generated turn cannot recursively
continue through this hook.

Posting an event or emitting a notice is **not** an agent read receipt. The hook
never claims or consumes mail. The model reads the mailbox through Mike and
acknowledges its own received messages explicitly.

## Bounds and limitations

The handler discards inputs larger than 1 MiB and strips prompts, tool arguments,
tool results, response bodies and transcript paths before posting. Its HTTP
request has a 600 ms deadline and the subprocess an 800 ms deadline. Failures
leave Codex running. There is no offline payload spool or replay loop; lifecycle
facts may be lost while Mike is unavailable and recover on later hooks.

Local dedup state holds at most 256 session files, expires after seven days,
and contains only a pending-ID digest and a turn ID. It lives under
`<CODEX_HOME>/mike/state` for installed hooks. Old spool files from the former
observe-only adapter are not replayed or automatically deleted.

Hooks alone do not wake an idle Codex session. `codex queue --thread <uuid>
--message <static-notice> --resume` can queue a notice and attempt a daemon
resume, but a successful exit does not prove the resume succeeded or the agent
read mail. The host must reserve notifications durably and validate local
Codex-home/source provenance before using that separate wake path. See the
[official CLI reference](https://learn.chatgpt.com/docs/cli/reference).

## Script regression checks

```bash
python3 -m unittest discover -s scripts/tests -p test_codex_hooks.py
```

The legacy `fixtures-from-recording` utility still converts explicitly recorded
hook/OTLP trials into scrubbed Rust fixtures. Recording full payloads is not the
normal installation path.
