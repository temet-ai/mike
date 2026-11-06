---
title: cargo sqlx prepare — developer workflow
status: active
created: 2026-04-30
updated: 2026-10-13
summary: Workflow for regenerating the .sqlx/ offline query cache after editing a sqlx query!/query_as!/query_scalar! macro call site.
---

# `cargo sqlx prepare` — developer workflow

The workspace's `sqlx::query()` (runtime-checked) call sites have migrated to
`sqlx::query!()` / `sqlx::query_as!()` / `sqlx::query_scalar!()` (compile-time-checked)
macros — 198 macro call sites across `crates/mike/src` as of this writing.
The `.sqlx/` offline cache holds ~200 checked-in query entries and is what makes
`cargo build`/`check`/`nextest` work with **no database and no `DATABASE_URL` env
var** (see CLAUDE.md "Build & Test Efficiency"). CI's `rust-quality.yml` workflow
enforces the cache stays in sync (`cargo sqlx prepare --workspace --check`,
`.github/workflows/rust-quality.yml`).

## What `cargo sqlx prepare` does

The sqlx `query!`/`query_as!` macros validate SQL at compile time by talking to a live Postgres instance. That's slow, fragile, and hard to fit into hermetic CI. The `cargo sqlx prepare` command replaces the live-DB requirement with a checked-in cache: it runs every macro once against the database you pass via `--database-url`, captures the schema-validated query metadata as JSON, and writes it to `.sqlx/`. Subsequent compiles read from `.sqlx/` instead of reaching out to Postgres.

The cache is git-tracked. CI's `cargo sqlx prepare --workspace --check` verifies that `.sqlx/` is in sync with the macros — if you forgot to regenerate after editing a query, CI catches it.

## When to run it

| Trigger | Action |
|---|---|
| Edited an existing `query!`/`query_as!`/`query_scalar!` body | `cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL"`, commit `.sqlx/` |
| Added a new macro call site | Same |
| Landed a migration that touched a column referenced by a macro | Same |
| Just running tests / no SQL touched | No action needed — the cache stays valid |

## Prerequisites

1. **A development PostgreSQL with the latest embedded schema applied.** Provision an owned instance with `mike --config PRIVATE_CONFIG database provision --port 6666`, then run `mike --config PRIVATE_CONFIG serve` to apply embedded migrations. The wrapper `scripts/local-db/up.sh --config PRIVATE_CONFIG` provisions only; it never replays SQL. The provider targets PostgreSQL 19 and builds the shared image from an embedded recipe with a digest-pinned PostgreSQL base. See [`INSTALLED_DATABASE.md`](../INSTALLED_DATABASE.md). Keep test fixtures isolated from capture data.
2. **`sqlx-cli` installed**:
   ```sh
   cargo install sqlx-cli --locked --no-default-features --features rustls,postgres
   ```
   (This matches the install line CI runs in `rust-quality.yml`.)
3. **Pass the DB URL on the command line.** The repo's only DB var is `MIKE_DATABASE_URL` — there is no `DATABASE_URL` env var. Hand it to sqlx with the `--database-url` flag:
   ```sh
   cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL"
   ```
   This must match the database URL the proxy is actually booting against, not the sample default.

## Developer workflow

When changing or adding a query inside a macro:

```sh
# 1. Make sure your local schema is current.
cargo run --bin mike -- migrate   # OR start the proxy once; it migrates on boot.

# 2. Modify the query in source.
# 3. Regenerate the cache from the workspace root:
cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL"

# 4. Stage and commit the changes alongside the source edit.
git add .sqlx/ <source-file>
git commit
```

The `--workspace` flag is essential — without it `prepare` only inspects the crate in the current directory and silently misses macros in sibling crates.

## Schema-change workflow

When landing a migration that touches a column referenced by a macro:

```sh
# 1. Apply the migration locally.
cargo run --bin mike -- migrate

# 2. Regenerate the cache.
cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL"

# 3. Commit migration + .sqlx/ together.
git add crates/mike/src/store/persistence/migrations/<new-migration>.sql .sqlx/
```

Committing a migration without regenerating `.sqlx/` is a bug: subsequent compiles fail when the macro's expected types diverge from the new column shape. The CI `--check` step catches this before merge.

## Troubleshooting

**`error: error returned from database: relation "..." does not exist`**
You're running `prepare` against an empty or stale DB. Apply migrations: `cargo run --bin mike -- migrate`.

**`error: VARIABLE not set: DATABASE_URL`**
Pass the URL on the command line instead of relying on an env var: `cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL"` (the repo uses `MIKE_DATABASE_URL`, not `DATABASE_URL`).

**`cargo build` fails with "no cached data for query" but you didn't change any SQL**
Pull the latest `.sqlx/` from main — someone else regenerated it after a schema change you didn't have locally.

**`cargo build` fails with "no cached data for query" right after editing source**
You forgot step 3. Run `cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL"` and commit the result.

**Macros work locally, fail in CI**
CI's `--check` flag enforces that `.sqlx/` is up-to-date. Locally you may have committed source without regenerating; check `git status .sqlx/` and re-prepare.

## Offline mode (CI / hermetic builds)

`cargo sqlx prepare` produces enough metadata for compile to succeed without a live DB. Set `SQLX_OFFLINE=true` in environments where Postgres is unavailable (CI runner, release build). If the cache is missing or stale, the compile will fail with a clear "no cached data" error — no silent fallback.
