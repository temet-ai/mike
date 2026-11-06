# proxy-forward

An HTTPS **forward proxy sidecar** that lets Claude Code keep Remote
Control (`ANTHROPIC_BASE_URL` unset) while temet-proxy still captures
Anthropic API traffic.

Recent Claude Code disables Remote Control when `ANTHROPIC_BASE_URL` points
anywhere other than `api.anthropic.com`. temet-proxy's capture pipeline
normally requires `ANTHROPIC_BASE_URL=http://127.0.0.1:9988`, forcing a
choice between capture and Remote Control. `proxy-forward` removes that
choice: Claude Code is launched with `ANTHROPIC_BASE_URL` **unset** and
`HTTPS_PROXY` pointed at this sidecar.

## How it works

```
Claude Code (HTTPS_PROXY set, ANTHROPIC_BASE_URL unset)
   │  CONNECT api.anthropic.com:443        CONNECT some.other.host:443
   ▼                                       ▼
proxy-forward :9987 ── host == api.anthropic.com ? ──────────────┐
   │ yes → selective MITM                  │ no → blind tunnel    │
   │  terminate client TLS with a          │  raw bidirectional   │
   │  CA-signed leaf, read /v1/* request   │  byte copy, opaque   │
   ▼                                       ▼
 route per failover state machine    upstream host:port (TLS stays opaque)
   │
   ├─ proxy Up AND path /v1/*  → http://127.0.0.1:9988  (temet-proxy captures)
   └─ otherwise                → https://api.anthropic.com (direct, bypasses capture)
```

- **Selective MITM.** Only `api.anthropic.com` is TLS-terminated, with an
  on-demand leaf certificate signed by a locally-generated CA. Every other
  `CONNECT` is **blind-tunnelled** — Remote Control's own traffic is never
  inspected or re-signed.
- **Failover.** A background prober polls temet-proxy's existing
  `/api/health` route. When it is up, `/v1/*` requests route through
  `:9988` so capture happens; otherwise (or for non-`/v1` paths) the
  sidecar re-originates TLS straight to Anthropic so traffic never breaks.
  The start state is **pessimistic** — until the first successful probe,
  traffic goes Direct.
- **HTTP/2.** The client-facing server offers ALPN `["h2","http/1.1"]` and
  serves whichever the client negotiates; the direct-Anthropic leg
  re-originates TLS with the same ALPN. The routed `:9988` leg is plain
  HTTP/1 (temet-proxy is an HTTP/1 reverse proxy).
- **HTTP upgrades (WebSocket / `101`).** Claude Code's Remote Control
  channel is a persistent WebSocket to `api.anthropic.com`, not a plain
  request/response call — without upgrade support, routing that connection
  through this sidecar silently drops it, defeating the whole point of the
  sidecar. An HTTP/1.1 `Connection: Upgrade` request is detected on the
  MITM leg and always forwarded **Direct**, never through the routed
  `:9988` leg (temet-proxy is a request/response reverse proxy and cannot
  carry a raw upgraded stream) — this holds even for a `/v1/*` path, which
  would otherwise route through `:9988`. Once the direct leg answers `101`,
  the client-facing and upstream raw byte streams are bridged
  bidirectionally; everything else (ordinary `/v1/messages` capture, blind
  tunnelling, failover) is unaffected.
  As of Claude Code 2.1.258 the Remote Control worker channel is no longer
  a WebSocket: it is plain HTTPS on `api.anthropic.com/v1/code/sessions/…`
  with a long-lived SSE `GET …/worker/events/stream` for phone → local
  delivery, which routes through `:9988` like any `/v1/*` call and is
  relayed frame by frame (pinned by
  `remote_control_sse_stream_relays_each_frame_through_routed_leg`; see
  `artifacts/2026-09-02/remote-control-reply-leg-resolved.md`).

This is a **separate binary** — it ships nothing into temet-proxy's
process, in the same decoupled mould as `crates/mike-mcp/`.

## Quick start

```bash
# 1. Build it
cargo build -p proxy-forward --release

# 2. One-time: trust the MITM CA. export-ca prints the CA PEM to stdout.
proxy-forward export-ca > /tmp/pf-ca.pem

# 3. Run the sidecar (defaults shown; all overridable via env or flags)
proxy-forward            # or: proxy-forward serve

# 4. Point Claude Code at it — Remote Control stays ON (ANTHROPIC_BASE_URL unset)
NODE_EXTRA_CA_CERTS=/tmp/pf-ca.pem \
HTTPS_PROXY=http://127.0.0.1:9987 \
claude
```

### Trusting the CA

`proxy-forward export-ca` writes the CA certificate (PEM) to stdout. Claude
Code (a Node process) trusts it either via:

- `NODE_EXTRA_CA_CERTS=/path/to/pf-ca.pem` (per-process, shown above), or
- importing the PEM into the OS trust store (system-wide).

> **Security.** This is a private MITM CA on **your own machine**. Anyone
> who can read `ca-key.pem` can forge `api.anthropic.com` for you, so the
> key is written `0600` and the proxy binds loopback-only by default. The
> CA only ever signs the single intercept host.

## Subcommands

| Command | Effect |
|---|---|
| `serve` (default) | Run the forward proxy. |
| `export-ca` | Print the MITM CA certificate PEM to **stdout** (logs stay on stderr, so the output is pipeable). |
| `print-config` | Print the resolved configuration to **stderr**. |

## Configuration

Resolution order per setting: **CLI flag > environment variable >
default**.

| Setting | Env var | Flag | Default |
|---|---|---|---|
| Bind address | `MIKE_FORWARD_BIND` | `--bind` | `127.0.0.1:9987` |
| Routed-leg target (temet-proxy) | `MIKE_FORWARD_UPSTREAM` | `--upstream` | `http://127.0.0.1:9988` |
| Intercept host (MITM'd) | `MIKE_FORWARD_INTERCEPT_HOST` | `--intercept-host` | `api.anthropic.com` |
| Health probe URL | `MIKE_FORWARD_HEALTH_URL` | `--health-url` | `http://127.0.0.1:9989/api/health` |
| Probe interval | `MIKE_FORWARD_PROBE_INTERVAL` | — | `2s` |
| Probe timeout | `MIKE_FORWARD_PROBE_TIMEOUT` | — | `1s` |
| Probe mode | `MIKE_FORWARD_PROBE_MODE` | — | `http` |
| CA directory | `MIKE_FORWARD_CA_DIR` | `--ca-dir` | `directories` data dir (e.g. `~/.local/share/proxy-forward/`) |
| Max buffered request body | `MIKE_FORWARD_MAX_BODY_BYTES` | `--max-body-bytes` | `67108864` (64 MiB) |
| Direct-leg extra CA | `MIKE_FORWARD_DIRECT_EXTRA_CA` | `--direct-extra-ca` | _(none — native roots only)_ |
| Direct-leg dial override | `MIKE_FORWARD_DIRECT_TARGET` | `--direct-target` | _(none — dial `intercept_host:443`)_ |
| Acknowledge insecure direct seams | `MIKE_FORWARD_ALLOW_INSECURE_DIRECT=1` | `--allow-insecure-direct` | _(off — the seams above are refused without it)_ |

The **health probe** is a **combined** check, because the inspect bind
(`:9989`, where `/api/health` lives) and the routed leg the router actually
dials (`:9988`, temet-proxy's proxy bind) are different ports and can
diverge. A probe reads **Up only when BOTH**:

1. an HTTP `GET` to `MIKE_FORWARD_HEALTH_URL` returns HTTP 200 with JSON
   `status == "ok"` (top-level status, parsed as JSON — a nested
   subsystem's `status` never shadows it), AND
2. a TCP connect to the routed leg's `host:port` (parsed from
   `MIKE_FORWARD_UPSTREAM`, i.e. `:9988`) succeeds within the timeout.

Anything else (non-200, unreachable, timeout, or a dead routed port) reads
Down. This requires no change to temet-proxy.

The **max buffered request body** (`MIKE_FORWARD_MAX_BODY_BYTES`) bounds
the request body the MITM path buffers so it is replayable across the
failover legs. A body larger than the cap is rejected with **HTTP 413
Payload Too Large** — a safety bound, never silent truncation. The default
(64 MiB) is generous for Anthropic bodies, including inline images.

### Insecure direct-leg seams (test / corporate-MITM only)

`MIKE_FORWARD_DIRECT_EXTRA_CA` adds an extra trust root to the
direct-Anthropic leg **in addition to** the OS native roots (for a
corporate-MITM environment). `MIKE_FORWARD_DIRECT_TARGET` redirects only
the *TCP destination* of the direct leg while still verifying the
`intercept_host` certificate (used by the integration tests against a local
TLS stub). Both are `None` in normal operation; neither weakens TLS
verification of the `intercept_host` certificate.

Because a redirected dial or an extra-trusted CA is nonetheless a MITM
surface, **either seam is refused at startup unless you also set
`MIKE_FORWARD_ALLOW_INSECURE_DIRECT=1` (or pass `--allow-insecure-direct`)**
to acknowledge it. This fail-closed gate stops a stale or leaked env var
from silently redirecting real Anthropic traffic. When the ack is present
and a seam is set, the proxy logs a prominent startup warning.

## Out of scope

Wiring this into `scripts/start-proxy.sh` and any `make install`-style
install of the binary is a **later** step, intentionally not part of this
crate.
