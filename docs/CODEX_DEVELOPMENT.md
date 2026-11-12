---
title: Developing the Codex client
status: active
created: 2026-11-12
updated: 2026-11-12
summary: The independent managed Codex client, its Mike integration boundary, and focused development checks.
---

# Developing the Codex client

`crates/mike-codex/` is the managed Codex app-server client used by Mike's fleet
launcher. It is a library in this workspace, with its own tests and no dependency
on Mike, SQLx, Axum or the frontend. The purpose of this boundary is a shorter
edit-and-test loop for the client.

## Ownership

The client owns the app-server connection, protocol request construction,
thread start/resume/fork, turn dispatch/interruption and returned turn results,
hook and OpenTelemetry decoding, and incremental Responses metadata parsing.
Mike owns fleet contracts and permissions, session registration, authentication
file setup, durable mail, persistence and reader policy. Mike's
`launch/codex/` module connects those responsibilities to the client.

Client tests use a local fake app-server and synthetic messages. They need no
Codex installation, credentials, model inference, database or running Mike.
The existing opt-in real-Codex tests remain host integration tests; a fake-server
test is not evidence that a new Codex version emits the same protocol.

## Focused checks

From the repository root:

```sh
scripts/cx test -p mike-codex
scripts/cx check -p mike-codex --lib
scripts/lint-rust -p mike-codex
```

Choose the appropriate narrow check for the change and run Rust lint after the
last edit, following [the project check workflow](GITHUB_WORKFLOWS.md).
Do not run every command above for every edit. Cargo still needs a host
integration build when Mike consumes a changed client; extraction does not
eliminate that cost.

The push gate checks changed workspace packages and their dependent consumers,
including development/build dependencies. Changes to workspace configuration or
unresolved source ownership retain conservative coverage. The changed-area test
runner selects packages and targets before filtering tests, so unrelated test
executables do not have to build for a focused adapter run.

The initial extraction was measured on 2026-11-12 in the existing checkout:

| Check | Elapsed | Result |
| --- | --- | --- |
| Existing managed-client tests in `mike` | 783.8 seconds | 8 passed |
| Extracted `mike-codex` suite | 9.3 seconds | 13 passed |
| Client and host Rust lint (`-p mike-codex -p mike`) | 502.2 seconds | Passed, including all-target Clippy compilation |

These timings include compilation. Cache state and shared-host load differed, so
these are observed command times, not a controlled speedup ratio or a measurement
of the complete delivery gate. The extracted suite includes subprocess lifecycle
tests; its one ignored entry is the fake app-server executable used by those
tests. The old host run excluded the database-backed thread-binding test.

## Integration boundary

The shared observation work for [#181](https://github.com/temet-ai/mike/issues/181)
keeps provider decoding in this crate. Mike maps those decoded facts to its common
session, model-call and item contracts and owns SQL admission, expiry, mail
policy and reader queries. HTTP routing and byte forwarding are host concerns;
provider-specific Responses parsing is tested in the client crate. No second
daemon, release pipeline or provider-specific reader stack is introduced.

Routine Codex observation retains bounded metadata without raw payloads. Wire
evidence is separately opt-in and bounded; see [capture policy](CODEX_CAPTURE.md).
Transport limits emit observation-gap signals rather than silently presenting
partial results as complete. Provider fixture and malformed/oversized-stream
tests run here. Host integration tests cover the boundary: identity, atomic
facts/retry handling, quotas, mail state and common reader results. They do not
repeat the entire provider protocol fixture matrix inside Mike.

When extending the client, keep its dependency tree independent of Mike even in
tests. Use the host tests to verify registration, mail and database integration;
do not pull those systems into the client test suite.

## Experimental network proxy

Codex CLI 0.167.2 exposes the experimental `network_proxy` feature. It controls
network access for sandboxed commands. Model requests, MCP connections and other
client service traffic use separate connections, as documented in
[OpenAI's permissions reference](https://learn.chatgpt.com/docs/permissions#what-the-network-proxy-does-not-control).
This feature may support a future command-network policy integration; session
observation still needs structured runtime signals. Enabling it does not provide
Mike with app-server lifecycle events or solve capture retention.
