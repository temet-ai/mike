---
title: Local access and browser trust
status: active
created: 2026-10-13
updated: 2026-10-13
summary: Exact local listener authorities, browser origin checks, native client access, and unsupported remote exposure.
---

# Local access and browser trust

The supported preview runs on one machine. The proxy defaults to
`127.0.0.1:9988`, the inspect API to `127.0.0.1:9989`, and the embedded browser UI
to `127.0.0.1:9990`. IPv6 loopback (`[::1]`) and custom ports are supported.
`localhost` binds resolve deterministically to IPv4 loopback. Listener configuration
is checked before database startup or background workers.

Captured prompts, responses, tool inputs, and tool results can contain sensitive
material. Local access controls are a browser boundary, not authentication: another
process or user able to access the machine's loopback sockets can use the API.

## Browser requests

Inspect and embedded UI requests must carry one valid HTTP Host authority. HTTP/2
may instead supply its request URI authority. If both are present they must match.
Accepted authorities are the listener's actual address and the exact local aliases
`localhost`, `127.0.0.1`, and `[::1]`, at that listener's actual port.

`MIKE_INSPECT_ALLOWED_HOSTS` adds explicit authorities. A hostname without a port
uses the listener's port; an authority with a port allows only that exact port.
A configured browser origin does not implicitly authorize its hostname as a Host.

An Origin must match an explicitly configured `MIKE_SPA_ORIGINS` entry or the
validated request authority on HTTP. Scheme, host, and effective port all count:
HTTP and HTTPS are different origins, while an omitted HTTP port equals port 80
and an omitted HTTPS port equals port 443. IPv6 textual equivalents compare by
address. The default browser origins are `http://127.0.0.1:9990` and
`http://localhost:9990`.

Null, malformed, duplicate, combined, suffix-confusable, and non-ASCII header
values are rejected. Paths, query strings, fragments, credentials, trailing dots,
and browser-style repaired IP spellings are not accepted as origins or Host
values. An invalid nonempty origin configuration does not fall back to permissive
defaults. Forwarded-Host and Forwarded-Proto headers do not change the policy.

The check happens before REST handlers, CORS processing, and WebSocket upgrades.
An Origin-less browser request with cross-site or same-site Fetch Metadata is
also rejected, including GET navigation requests. Approved same-origin browser
requests and configured UI cross-origin requests continue to work.

## Native clients and proxy traffic

CLI and MCP clients do not need an Origin. They must use a valid Host authority
(or an HTTP/2 authority) matching the listener. Missing, duplicate, or malformed
Host headers are not a native-client escape hatch. Normal HTTP clients provide
Host automatically.

The capture proxy's `/v1/*` and `/v2/*` traffic does not use this browser guard.
It continues to capture the original client bytes and preserve the normal
pass-through behavior. Browser protection does not impose a JSON-only or other
content-type restriction on captured traffic.

## Unsupported remote compatibility

Non-loopback IP binds, including `0.0.0.0` and `[::]`, are refused by default for
all enabled proxy, inspect, and embedded UI listeners. Existing deployments that
intentionally need remote listeners must explicitly set
`MIKE_UNSUPPORTED_ALLOW_REMOTE_LISTENERS=1` or
`--unsupported-allow-remote-listeners 1`. This is outside the supported preview
and does not add authentication or TLS. Remote Host authorities still need to
match the configured listener or explicit allowed-host list. Arbitrary DNS names
are not accepted as bind addresses.

This option changes only mike's listener policy. It does not expose PostgreSQL,
change firewall rules, or change OS network settings. The development forwarding
sidecar already defaults to `127.0.0.1:9987`; an explicitly remote sidecar bind
warns that it can tunnel to machine-local services. Keep managed database and
sidecar listeners on loopback as well.

## Local regression checks

Run `scripts/cx test -p mike --test integration local_access` for policy tables
and real-socket tests of both inspect and embedded UI ports. The tests use an
in-memory capture store and synthetic upstream data, without a live database.
They cover rejected REST access and upgrade attempts, working native WebSockets,
synthetic capture delivery to approved clients, and request/response byte identity
through the capture proxy.

Run `node scripts/test-local-access-browser.mjs /path/to/mike` on a host with
Chromium and Node 22 or later for the isolated real-browser regression. It starts
an in-memory mike with temporary configuration, serves synthetic upstream traffic,
uses a fresh Chromium profile, and removes its processes and temporary files when
the run ends. It requires Chromium and mike to share loopback access. A
`--manual-browser` option prints bounded test-page URLs for a supported browser
when direct browser process startup is unavailable. No database is used; the
approved mutation control returns 503, and the Rust middleware test separately
proves rejected mutation handlers are never entered.

The harness emits binary hash/version, source revision, browser version, and each
origin/port outcome. A browser startup or network failure is NOT RUN, never a
security pass.

These tests do not replace the release gate's independent security verification.
Record the real-browser allowed, unrelated, and null-origin results, including
no synthetic event delivery to rejected clients, before marking AC06 complete.
