---
title: Offline Development
status: active
created: 2026-06-21
updated: 2026-10-13
summary: How to compile, lint, and test mike without internet access or a reachable remote Postgres.
---

# Offline Development

How to develop, debug, and test `mike` with **no internet and no Claude API** — on a
plane, in a café, anywhere `api.anthropic.com` is out of reach.

The short version: compile, lint, and the fast unit-test tier all work fully offline with no
database at all. Running the proxy itself is different — persistence defaults **ON**
(`persist_enabled` defaults `true`) and `scripts/start-proxy.sh` unconditionally preflights for a
reachable database before it will boot, even if you intend in-memory mode. The offline story is
therefore the **local-DB fallback**: a Postgres you run entirely on your own machine (see
[Local DB fallback](#running-the-proxy-offline-local-db-fallback) below and
[`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md)), not an in-memory mode. DB-backed integration and
e2e tests require a reachable Postgres at `MIKE_TEST_ADMIN_URL` — they cannot run offline at all.

---

## Before you leave (these need internet — do them now)

| Step | Command | Why |
|---|---|---|
| 1. Warm the crate cache | `cargo build --all-targets` | Pulls every dependency **and dev-dependency** (wiremock, proptest, nextest deps) into `~/.cargo/registry`. |
| 2. Confirm tooling | `cargo nextest --version && rust-analyzer --version` | Both are local binaries; just verify they exist. |
| 3. Sanity-check offline | `CARGO_NET_OFFLINE=true cargo build --all-targets --offline` | If this succeeds, you are ready. It fails *fast* if a crate is missing. |
| 4. Export a replay fixture | `scripts/offline/export-fixture.sh` | Snapshots a real captured session from the remote proxy DB so you can drive the whole system offline (see [Replay a real session](#replay-a-real-session-offline)). |

The `.sqlx/` query cache (~200 queries) is committed. sqlx compile-time macros verify against
this cache with no DB connection — `cargo build` offline compiles normally without setting any
DB env var, and there is no `DATABASE_URL` in this repo to set.

---

## What works offline

| Activity | Command | Needs |
|---|---|---|
| Compile workspace | `cargo build --all --offline` | nothing |
| Lint (stringent) | `cargo clippy --all-targets -- -D warnings` | nothing |
| Format | `cargo fmt` | nothing |
| **Proxy tests** (the 9 mandatory) | `cargo nextest run -p mike --offline` | nothing — wiremock + raw TCP on localhost |
| Run the proxy offline | `mike database provision` (pinned image already cached), then `mike serve` | a local Postgres — see [below](#running-the-proxy-offline-local-db-fallback) |
| Replay a captured session | see [Replay section](#replay-a-real-session-offline) | fixture exported while online |

---

## What does NOT work offline

| Activity | Why |
|---|---|
| DB-backed integration / e2e tests | `MIKE_TEST_ADMIN_URL` must reach a live Postgres admin database; the test helper creates ephemeral clones from a migrated template |
| Forward to `api.anthropic.com` | No internet |
| `cargo sqlx prepare` to regenerate the query cache | Requires `MIKE_DATABASE_URL` to point at a running Postgres |

If you need to regenerate the `.sqlx` cache after changing a query, you must be online (or
on a network that can reach your Postgres):

```bash
cargo sqlx prepare --database-url "$MIKE_DATABASE_URL"
```

---

## Running the proxy offline (local-DB fallback)

Persistence defaults **ON**. With an installed or locally built candidate binary and its
pinned image already cached, use the same owned provider as online setup:

```bash
mike database provision --port 6666
mike serve
```

The first command saves the selected private config and provisions storage only; `serve`
applies embedded migrations. Docker and Compose must be available. There is no automatic
native `initdb` or `/tmp` fallback. An existing local PostgreSQL 19 is another explicit
choice; see [`INSTALLED_DATABASE.md`](INSTALLED_DATABASE.md).

Using a separate local database as a fallback for a remote primary remains an explicit
advanced configuration. Set `local_database_url` deliberately and follow
[`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md), including its fresh-fallback cursor step.
The provider does not silently enable fallback or overwrite the primary installation.

For ephemeral capture without any database, explicitly select trial mode or launch
`MIKE_PERSIST_ENABLED=0 mike serve`. History is lost when that process stops. The
source-development `start-proxy.sh` workflow has its own database preflight and is not
needed for the installed binary.

---

## Replay a real session offline

The proxy normally forwards to `api.anthropic.com`. Offline, you replace that upstream with a
**replay of a real captured session** — so the entire pipeline (forward → SSE tee → capture →
inspect API → SPA) runs on real data with no internet. Three scripts under `scripts/offline/`
(fixtures land in the gitignored `scripts/offline/fixtures/`):

```bash
# 1. ONLINE (while MIKE_DATABASE_URL is reachable): snapshot a real session.
#    Defaults to the most recent complete request; pass an id to pick one.
scripts/offline/export-fixture.sh                 # → scripts/offline/fixtures/<id>/

# 2. OFFLINE: serve that session as a fake upstream on :9099 (stdlib python, zero build).
python3 scripts/offline/mock-upstream.py --fixture scripts/offline/fixtures/<id> --delay-ms 25 &

# 3. OFFLINE: with the proxy running (MIKE_UPSTREAM_URL=http://127.0.0.1:9099),
#    push the recorded request through it.
scripts/offline/replay.sh scripts/offline/fixtures/<id>
```

`mock-upstream.py` rebuilds the full Anthropic SSE framing (`message_start` → content blocks →
`message_stop`) from the stored response deltas; `--delay-ms` paces the events so you can watch
the response stream into the SPA live. The replayed request lands as a new captured request
in the inspect API (`http://127.0.0.1:9989/api/requests`) and SPA (`http://localhost:9990/`),
indistinguishable from live traffic.

The proxy binary accepts `MIKE_UPSTREAM_URL=http://127.0.0.1:9099` to redirect `/v1` traffic
to the mock. The inspect bind (`:9989`) works normally; capture needs the same reachable DB as
any other offline run (see [above](#running-the-proxy-offline-local-db-fallback)) unless you
launch the binary directly with `MIKE_PERSIST_ENABLED=0` for ephemeral in-memory capture.

---

## Debugging a single test under breakpoints

nextest does not attach the debugger to individual tests. For step-through debugging of one
test, use a plain-`cargo-test` config (RustRover offers a gutter ▷/🐞 icon beside every
`#[test]` / `#[tokio::test]`), or from the CLI:

```bash
cargo test --offline -p mike <test_name> -- --nocapture
```

DB-backed tests will skip when `MIKE_TEST_ADMIN_URL` is unset — that is correct behaviour
offline.

---

## Coming back online

```bash
unset CARGO_NET_OFFLINE
```

`MIKE_DATABASE_URL` and `MIKE_TEST_ADMIN_URL` should already point at your remote Postgres
via `.env`. If you overrode them for any local experiment, restore from `.env` or
`source .env`.
