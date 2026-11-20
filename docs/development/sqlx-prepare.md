---
title: The .sqlx offline cache
status: active
created: 2026-04-30
updated: 2026-11-20
summary: What the git-tracked .sqlx/ offline query cache holds and what scripts/sqlx-prepare does to it.
---

# The `.sqlx/` offline cache

The `sqlx::query!` / `query_as!` / `query_scalar!` macros check their SQL against a live
Postgres at compile time; the git-tracked `.sqlx/` cache holds one described entry per macro
so `cargo build`, `check` and `nextest` work with no database.

`scripts/sqlx-prepare [--db <url>] [--update-changed] [--prune] [-- <cargo check args>]`
describes the tree's queries into a temporary folder and copies in only what is new. By default
it describes against `MIKE_DATABASE_URL` from `.env` (the capture database; describing changes
nothing in it). `--update-changed` also replaces entries whose description differs from the
stored one; `--prune` also deletes entries no query produces any more. Its last line is
`SQLX-PREPARE: PASS|FAIL new=<n> changed=<n> stale=<n>`.

Raw `cargo sqlx prepare` empties `.sqlx/` while it runs.

Migrations are applied only by `mike serve`; there is no `mike migrate` subcommand, and
`mike migrate-check` only compares the ledger with the binary.
