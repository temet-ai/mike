---
title: Developing the Codex client
status: active
created: 2026-11-12
updated: 2026-11-23
summary: The independent Codex client and its Mike integration boundary.
---

# Developing the Codex client

`crates/mike-codex/` owns the Codex app-server client, observation decoders and
interactive argument/setup adapters. It is a library in this workspace, with its own tests and no dependency
on Mike, SQLx, Axum or the frontend. The purpose of this boundary is a shorter
edit-and-test loop for the client.

## Ownership

The client owns the app-server connection, protocol request construction,
thread start/resume/fork, turn dispatch/interruption and returned turn results,
hook and OpenTelemetry decoding, and incremental Responses metadata parsing.
Mike owns fleet contracts and permissions, session registration, authentication
file setup, durable mail, persistence and reader policy. Mike's
`launch/codex/` module connects those responsibilities to the client.

Ordinary `mike spawn open <folder> --runtime codex` uses the same terminal backend
as other launches. Pure interactive Codex arguments and bounded setup execution
live in `mike-codex`; the host locates the packaged installer, records launch identity
and opens the terminal. Hook ingestion validates the launch's session and home
before binding the native thread. Telemetry and child hooks arriving before that hook are marked
`awaiting_spawn_hook` and is not spooled or assigned a second session identity.

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

Routine Codex observation retains bounded metadata without raw payloads. Wire
evidence is separately opt-in and bounded; see [capture policy](CODEX_CAPTURE.md).
Transport limits emit observation-gap signals rather than silently presenting
partial results as complete. Provider fixture and malformed/oversized-stream
tests run here. Host integration tests cover the boundary: identity, atomic
facts/retry handling, quotas, mail state and common reader results. They do not
repeat the entire provider protocol fixture matrix inside Mike.

## Experimental network proxy

Codex CLI 0.167.2 exposes the experimental `network_proxy` feature. It controls
network access for sandboxed commands. Model requests, MCP connections and other
client service traffic use separate connections, as documented in
[OpenAI's permissions reference](https://learn.chatgpt.com/docs/permissions#what-the-network-proxy-does-not-control).
This feature may support a future command-network policy integration; session
observation still needs structured runtime signals. Enabling it does not provide
Mike with app-server lifecycle events or solve capture retention.
