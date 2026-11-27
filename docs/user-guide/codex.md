# Codex sessions

mike records complete OpenAI Codex requests and responses and delivers pending Mike mail and context through the proxy. Codex sessions appear beside your Claude Code sessions, with captured content available in the session view.

Codex sends its model traffic through mike, which forwards it to OpenAI. Capture is stored locally.

## What mike sees

For a Codex session routed through mike, capture includes:

- Complete request and response content, including prompts, answers, tool calls and tool results present in that traffic.
- Which model answered and how many tokens it used.
- Separate request occurrences and child-session identities, with repeated content sharing storage.

Open a session and use its capture list to read or download a request or response. Content is loaded when opened. Interrupted responses are labelled, and captured content is not discarded because a metadata parser cannot interpret it.

Capture has no new automatic expiry or storage quota. Repeated content is referenced instead of copied. A brief submitted through the launcher is also retained in its launch record.

## Connect Codex

To open a Codex terminal through mike:

```sh
mike spawn open /path/to/project --runtime codex
```

This prepares Mike MCP, removes Mike-owned hooks, and launches Codex through
mike's proxy. It preserves your Codex approval and
sandbox settings. To use the same approval bypass as `cx`, pass it explicitly:

```sh
mike spawn open /path/to/project --runtime codex -- --dangerously-bypass-approvals-and-sandbox
```

The `spawn_session` tool accepts `runtime: "codex"` and an optional `args` array.
Claude remains the default when runtime is omitted. Codex launches support fresh
interactive sessions on Herdr-core, tmux or a desktop terminal; warm templates and
forks still require Claude. Legacy Herdr does not support Codex. Select a supported
backend before the subcommand, for example
`mike --launch-backend tmux spawn open /path/to/project --runtime codex`.
Operator telemetry is preserved. The older Mike-installed exporter is disabled;
session identity comes from the proxy.

The launch returns Mike's session ID. Its proxy URL binds the native Codex
thread to that same ID when model traffic arrives. Opening a terminal does not confirm that the
initial prompt was answered. Codex has no Claude Remote Control channel, so the
launcher does not wait for one.

You can also source `scripts/shell/cdx.zsh` and use `cdx` or `cx`. Both send model
traffic through the proxy; `cx` also adds Codex's explicit approval bypass. These
wrappers report when Mike is unavailable and a launch would be uncaptured.

Capture and session identity do not require hook installation or hook trust.
Unrelated hooks and native Codex approval settings remain yours. The proxy is the
capture and injection boundary.

## Mail and context

Codex can send mail with `send_session_message` without supplying `from`.
Mike identifies the caller from the Codex thread and uses its Mike session ID.
Child sessions have their own sender identity. An explicit sender that disagrees
with the calling session is refused.

Mike adds pending mail and reminders to eligible model requests as external context.
The session view retains both the original request and the outgoing request so you
can inspect what Mike added. Peer content does not grant operator approval or change
Codex permissions.

For an open Codex session launched through Mike, a native queue notice can wake the
idle terminal. The proxy delivers the actual mail on its next request. The notice
contains no peer message body. A queued notice, delivered mail and acknowledged mail
are separate states: Mike records delivery against the captured outgoing request,
and the recipient explicitly acknowledges the message after handling it.

If the native notice fails, mail remains pending for the next model request. Older
Codex versions without native queue support still receive mail on an eligible request.

## Check capture

Run `mike doctor`, send a prompt in the launched session, then open that session in
Mike. The capture list shows each exchange and its completion status.
Capture needs no switch.

For compressed requests, Mike keeps the complete decoded payload and the original
headers. It forwards the original bytes when no context is added; when Mike adds
context, the outgoing decoded JSON is captured separately. If decoding fails, it
retains the encoded data and records the failure. Unknown fields and events remain
in the captured body. Binary content can be downloaded.

Only traffic sent through the proxy is captured. Local tool results appear when
Codex includes them in a model request; local work that produces no model traffic
is not visible through this route.

## When it does not work

| What you see | What to do |
|---|---|
| The launched session has no captures | Check that mike is running and that the session uses Mike's proxy URL, then send a prompt. |
| Mail cannot resolve the Codex thread to a Mike session | Check that model traffic goes through Mike and that captures appear. After updating Mike MCP, reconnect that server or start a new session. |
| A body is marked interrupted | The connection ended before the complete body arrived; the received bytes are retained. |
| A body downloads instead of displaying text | It contains binary or encoded data; the download preserves it. |

[Privacy](privacy.md) says what mike stores, and [Troubleshooting](troubleshooting.md) covers the rest.
