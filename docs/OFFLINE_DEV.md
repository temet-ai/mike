---
title: Offline Development
status: active
created: 2026-06-21
updated: 2026-11-06
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
e2e tests require a reachable Postgres admin database at `MIKE_TEST_ADMIN_URL`; a throwaway one on your own machine (`scripts/test-db/up.sh`, port 6667, image already pulled) works offline.

---

## Before you leave (these need internet — do them now)

| Step | Command | Why |
|---|---|---|
| 4. Cache the database images | `docker pull` the images `mike database provision` and `scripts/test-db/up.sh` use | Provisioning and the test database need the images already local. |

The `.sqlx/` query cache is committed. sqlx compile-time macros verify against
this cache with no DB connection — `cargo build` offline compiles normally without setting any
DB env var, and there is no `DATABASE_URL` in this repo to set.

---

## What works offline

| Activity | Command | Needs |
|---|---|---|
| Run the proxy offline | `mike database provision` (pinned image already cached), then `mike serve` | a local Postgres — see [below](#running-the-proxy-offline-local-db-fallback) |

---

## What does NOT work offline

| Activity | Why |
|---|---|
| DB-backed integration / e2e tests without a local Postgres | `MIKE_TEST_ADMIN_URL` must reach a live Postgres admin database; the test helper creates ephemeral databases on it |
| Forward to `api.anthropic.com` | No internet |
| `scripts/sqlx-prepare` to refresh the query cache | Requires `MIKE_DATABASE_URL` to point at a running Postgres |

If you need to regenerate the `.sqlx` cache after changing a query, you must be online (or
on a network that can reach your Postgres):

```bash
scripts/sqlx-prepare
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

## Pointing the proxy at a stand-in upstream

`MIKE_UPSTREAM_URL` (default `https://api.anthropic.com`) redirects `/v1` traffic. Offline, point it at
a local server of your own that speaks the Anthropic SSE framing, and the whole pipeline
(forward → SSE tee → capture → inspect API → SPA) runs with no internet. The wiremock-based tests
under `crates/mike/tests/proxy/` show the request and response shapes to serve. The inspect bind
(`:9989`) works normally; capture needs the same reachable database as any other offline run (see
[above](#running-the-proxy-offline-local-db-fallback)) unless you launch the binary directly with
`MIKE_PERSIST_ENABLED=0` for ephemeral in-memory capture.

---

## Coming back online

```bash
unset CARGO_NET_OFFLINE
```

`MIKE_DATABASE_URL` and `MIKE_TEST_ADMIN_URL` should point at your usual Postgres. If you
overrode them for a local experiment, restore the previous values.
