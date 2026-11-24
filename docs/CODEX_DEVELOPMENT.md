---
title: Developing the Codex client
status: active
created: 2026-11-12
updated: 2026-11-24
summary: The independent Codex client and its Mike integration boundary.
---

# Developing the Codex client

`crates/mike-codex/` owns the Codex app-server client, observation decoders and
interactive argument/setup adapters and lossless body chunking. It is a library in this workspace, with its own tests and no dependency
on Mike, SQLx, Axum or the frontend. The purpose of this boundary is a shorter
edit-and-test loop for the client.

## Ownership

The client owns the app-server connection, protocol request construction,
thread start/resume/fork, turn dispatch/interruption and returned turn results,
legacy hook and OpenTelemetry decoding, incremental Responses metadata parsing, and
complete byte segmentation for capture, native queue invocation and streaming Responses
input inspection/splicing. The splice adapter owns no mailbox or database policy.
Mike owns fleet contracts and permissions, session registration, authentication
file setup, durable mail, persistence and reader policy. Mike's
`launch/codex/` module connects those responsibilities to the client.

Ordinary `mike spawn open <folder> --runtime codex` uses the same terminal backend
as other launches. Pure interactive Codex arguments and bounded setup execution
live in `mike-codex`; the host locates the packaged installer, records launch identity
and opens the terminal. A launch-specific proxy URL binds the native thread to the
reserved Mike session. Setup registers MCP and removes Mike-owned hooks and the old Mike metadata exporter,
while preserving unrelated hooks, operator telemetry and Codex approval settings. Capture and identity do not
wait for hook trust or telemetry.

Client tests use a local fake app-server and synthetic messages. They need no
Codex installation, credentials, model inference, database or running Mike.
The existing opt-in real-Codex tests remain host integration tests; a fake-server
test is not evidence that a new Codex version emits the same protocol.

## Integration boundary

The shared observation work for [#181](https://github.com/temet-ai/mike/issues/181)
keeps provider decoding in this crate. Mike maps those decoded facts to its common
session, model-call and item contracts and owns SQL admission, expiry, mail
policy and reader queries. HTTP routing and byte forwarding are host concerns;
provider-specific Responses parsing is tested in the client crate. No second
daemon, release pipeline or provider-specific reader stack is introduced.

The proxy captures complete request and response bodies. Provider interpretation is
an independent projection: unknown fields, binary data and parser gaps remain
captured. The store uses direct references to previous chunks and returned surrogate
IDs, with no hash lookup or recursive reconstruction. It introduces no content expiry
or quota. See [capture storage](CODEX_CAPTURE.md).

Keep checks at their owning layer: chunking and protocol fixtures in `mike-codex`,
identity and document round trips in focused runtime tests, and a local upstream
fixture for unchanged forwarding, injected original/sent bytes, accepted delivery receipts,
rejected-request retry and recovery. Native queue checks use an isolated Codex home and
a local upstream; a successful queue command is never treated as consumption. Do not repeat the provider
fixture matrix in the host or require a running model for these checks.

## Experimental network proxy

Codex CLI 0.167.2 exposes the experimental `network_proxy` feature. It controls
network access for sandboxed commands. Model requests, MCP connections and other
client service traffic use separate connections, as documented in
[OpenAI's permissions reference](https://learn.chatgpt.com/docs/permissions#what-the-network-proxy-does-not-control).
This feature may support a future command-network policy integration; session
observation still needs structured runtime signals. Enabling it does not provide
Mike with app-server lifecycle events or solve capture retention.
