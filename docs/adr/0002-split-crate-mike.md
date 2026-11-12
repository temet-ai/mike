---
title: Split crate mike by breaking its dependency cycle first
status: proposed
created: 2026-11-12
updated: 2026-11-12
summary: Crate mike is one compilation unit of about 514k lines, and 99% of it sits in a single dependency cycle of 89 top-level modules, so no module can move into its own crate yet. The split proceeds by inverting back-edges, measured with scripts/module-graph, until store and the layers above it can leave as crates; each slice is small, lands on its own and is graded by the cycle shrinking.
---

# ADR 0002 — Split crate mike by breaking its dependency cycle first

**Status: proposed.** Nothing below is built yet; the measurements are from
`main` at `c819d9c3` (12 November 2026).

## Context

Crate `mike` holds about 514k lines in one compilation unit. Every edit
recompiles all of it and relinks every test harness; there is no parallelism
inside one crate, sccache cannot cache it (incremental path crate), and mbx
restores it only whole. Splitting it into workspace crates is the largest
remaining build-speed lever after the tooling in PR #184 (mbx by default, no
recompile on commit).

`scripts/module-graph` reads every `crate::<module>` path and reports the
graph between the 111 top-level modules of `crates/mike/src`:

| Measure | Value |
|---|---|
| Largest dependency cycle | 89 of 111 modules, 509,049 of 514,338 lines (99%) |
| Modules outside it | small leaves (`types`, `owner_only_file`, `circuit_breaker`, `work_items`, …) and a few workers only `serve` uses, about 5k lines |
| `store` (101,664 lines) uses | 35 other modules; `metrics` 917 times, `types` 47, `runtime` 26, `extract` 24, `launch` 23, `compose_prompt` 20 |
| `api` uses | `store` 641 times, `config` 213, `launch` 104 |
| `config` uses | `store` 81 times, `llm` 35, `steerer` 25, `dossier` 25 |

A crate cannot depend on a crate that depends on it, so any module in the
cycle that moves out takes the whole cycle with it. Peeling from the top
(modules nothing else uses) frees about 6k lines; peeling from the bottom
(leaves) about 5k. Neither changes what an edit to `store`, `launch` or `api`
recompiles.

## Decision

Split in two stages, and grade every slice with `scripts/module-graph`.

**Stage 1 — break the cycle.** Invert the back-edges that make the core depend
on the layers above it, one edge per slice, cheapest first:

1. `store → metrics` (917 references, 914 of them `metrics::time_db`). `time_db`
   needs only `metrics/names.rs`: move `db_timer`, `phase_timer` and `names`
   into a leaf crate `mike-metrics`, re-exported as `crate::metrics` so no call
   site changes. `gauges` and `install` stay in `mike`.
2. `store → types`, `store → work_items`, `store → lesson_shape`: already leaves;
   they move into a `mike-core` crate with the other leaves (`owner_only_file`,
   `circuit_breaker`, `cache_scope`, `environment`, `sandbox`, `logged`, …).
3. `store → launch`, `store → steerer`, `store → dossier`, `store → eval`,
   `store → compose_prompt`, `store → extract`, `store → analysis`: each is a
   type or function the persistence layer borrows from a feature. Move the
   borrowed type down into the store layer (or into `mike-core`), or invert it
   behind a trait the feature implements. `scripts/module-graph deps store` lists
   them with counts.
4. `config → store` (81) and `config → llm`, `steerer`, `dossier`: configuration
   should be data the layers read, not code that reaches into them. Move the
   loaders that touch the database into `store` or the feature that owns them.

A slice is done when `scripts/module-graph deps <module>` no longer lists the
edge and the crate still builds and passes its tests.

**Stage 2 — move layers out.** Once `store` and what it uses form no cycle with
the rest, extract crates bottom-up:
`mike-core` → `mike-metrics` → `mike-store` (with the migrations, `.sqlx/` and the
DB test fixtures) → `mike-config` → feature crates (`mike-launch`, `mike-steerer`,
`mike-api`, `mike-tui`, …) → the thin `mike` binary. Add `cargo-hakari` when
there are three or more crates so feature unification does not rebuild them
differently.

## Consequences

- Each slice is a small PR graded by a number. The cycle can only shrink, so
  progress is visible and the work can pause at any slice.
- Stage 1 changes no behaviour and no public surface; it moves types and
  inverts calls. It needs the DB tier (`make test-db`), because `store` is the
  layer that moves.
- `pub(crate)` items a later crate needs become `pub`. `#[cfg(test)]` helpers
  that other modules' tests use (the DB fixtures in `store::persistence::
  test_support`) move behind the existing `test-seams` feature so other crates'
  tests can reach them.
- The test harnesses in `crates/mike/tests/` stay with the binary until the
  crate they exercise has moved; the tier filters in `.config/nextest.toml`
  and `.config/test-tiers.json` are keyed by binary and module names and move
  with them.
- Until Stage 2 starts, the build-speed work is the tooling in PR #184.
