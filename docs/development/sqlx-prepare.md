---
title: Refreshing the .sqlx offline cache
status: active
created: 2026-04-30
updated: 2026-11-06
summary: How to regenerate the .sqlx/ offline query cache after editing a sqlx query!/query_as!/query_scalar! call site — always through scripts/sqlx-prepare, never raw cargo sqlx prepare.
---

# Refreshing the `.sqlx/` offline cache

The `sqlx::query!` / `query_as!` / `query_scalar!` macros check their SQL against a live
Postgres at compile time; the git-tracked `.sqlx/` cache holds one described entry per macro
so `cargo build`, `check` and `nextest` work with no database. Edit a macro's SQL, add a call
site, or land a migration that touches a column a macro reads, and the cache must be
refreshed and committed with the change. The one supported way is:

```sh
scripts/sqlx-prepare [--db <url>] [--update-changed] [--prune] [-- <cargo check args>]
```

It describes the tree's queries into a temporary folder and copies in only what is new, so
it is safe while other sessions work in the checkout. By default it describes against
`MIKE_DATABASE_URL` from `.env` (the capture database; describing changes nothing in it).
`--update-changed` also replaces entries whose description differs from the stored one;
`--prune` also deletes entries no query produces any more, only when no other session has
uncommitted queries. Its last line is `SQLX-PREPARE: PASS|FAIL new=<n> changed=<n> stale=<n>`.

Never run raw `cargo sqlx prepare`: it empties `.sqlx/` while it runs, so every other
session in the checkout sees hundreds of deleted files and cannot commit or build offline
until it ends (AGENTS.md, #405).

Migrations are applied only by `mike serve`; there is no `mike migrate` subcommand, and
`mike migrate-check` only compares the ledger with the binary. For a query that reads a
column that only an undeployed migration adds, pass `--db` with a scratch database a `mike
serve` of this build has migrated, such as the test database from `scripts/test-db/up.sh`
(`MIKE_TEST_ADMIN_URL`, `127.0.0.1:6667`). Tests never touch the capture database.

In CI, `.github/workflows/rust-quality.yml` installs `sqlx-cli`, runs `cargo sqlx migrate run` against
its CI database and then `cargo sqlx prepare --workspace --check -- --all-targets`, so a stale
cache fails the PR.
