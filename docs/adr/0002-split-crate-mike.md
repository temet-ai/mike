---
title: Split crate mike into peer crates along measured seams
status: proposed
created: 2026-11-12
updated: 2026-11-12
summary: Crate mike (517k lines) is one compilation unit and 99% of it one dependency cycle. A layered split saves little because persistence, at the bottom, changes in 45% of commits; the split that pays makes launch, fleet, runtime, knowledge and lessons peer crates over a thin base. The untangling happens inside the one crate first, graded by scripts/crate-plan against a committed target map, and a group becomes a crate only when nothing below it reaches up into it. Stage S5 (9 crates, 705 references cut) replays history at 61% of today's recompiled lines.
---

# ADR 0002 — Split crate mike into peer crates along measured seams

**Status: proposed.** Measured on `main` at `9c8c7bab` (12 November 2026, after
PR #185). Nothing below is built yet. The target map is
[`0002-crate-map.json`](0002-crate-map.json); `scripts/crate-plan` grades the
code against it and `scripts/module-graph` reports the cycle.

## Context

| Measure | Value |
|---|---|
| Crate `mike` | 516,768 lines, 111 top-level modules, one compilation unit |
| Largest dependency cycle | 89 modules, 511,479 lines (99%) |
| Incremental `cargo check -p mike --lib` after a one-line edit | 32–37 s (4-core container) |
| Rebuild of the lib test harness after a one-line edit | 114 s |
| Commits since 6 Nov touching `store/persistence` | 69 of 154 (45%); `migrations` 26 |
| `PersistHandle` | 350 `impl PersistHandle` blocks in about 210 files; 1,177 methods |

Every edit recompiles the whole crate and relinks every harness; there is no
parallelism inside one crate's front end; sccache cannot cache it and mbx can only
restore it whole.

## What the measurements say

1. **`store` is not a layer.** `store/persistence` (95k lines) is every feature's
   SQL, written as inherent methods on one type. 57k of those lines are shared by
   several features; `api` calls nearly every group.
2. **The feature modules are almost acyclic.** Without the hubs (`store`, `config`,
   `settings`, `api` and the app wiring), only 118 of 1,584 references between
   features (7%) point the wrong way. The hubs make the cycle.
3. **`config` and `settings` are three things each:** configuration data that every
   feature reads, the `AppState` wiring that aggregates every feature, and CLI
   subcommands (`config/subcommands`) and runtime services. The data belongs low,
   the rest at the top.
4. **A layered split saves little.** A crate change rebuilds every crate above it.
   With persistence at the bottom and changing in 45% of commits, base | features |
   top replays at 77–81% of today.
5. **Peers pay.** `launch/fleet` alone is 61k lines in 269 files and most of
   `launch`'s churn; making `launch` and `fleet` peers of the other features is the
   largest single step (80% → 68%). After S5 the curve is flat: the full 18-crate
   map cuts 169 more references and replays the same 61%.
6. **The ceiling is about 55%,** reached only if every feature-to-feature edge is
   cut, including legitimate ones (`fleet` builds on `launch`).

Lines recompiled is a proxy. It understates the gain in two ways: unchanged
crates restore from mbx (a single crate never can), and each crate's tests build
on their own (today every edit rebuilds the 114 s harness of the whole crate). It
overstates it in one: incremental compilation already skips unchanged codegen
units. Calibrate with `cargo build --timings` after S1.

## Decision

### Target (stage S5, 9 crates)

| Crate | Contents | Lines |
|---|---|---|
| `mike-base` | `core`, `wire`, `model`, `db`, `llm` of the map: shared types, metrics, request parsing and classifiers, configuration and settings data, `PersistHandle` with the capture path and the shared reads, LLM clients and `EmbeddingService` | ~73k |
| `mike-runtime` | `runtime` and its persistence | ~13k |
| `mike-launch` | session spawning, backends, mailbox, work threads, session notes, their persistence and API handlers | ~60k |
| `mike-fleet` | `launch/fleet`, `change_record`, their persistence and API handlers | ~82k |
| `mike-knowledge` | project knowledge, semantic, rerank, embeddings worker, search events | ~38k |
| `mike-features` | `proxy`, `dossier`, `steerer`, `eval` | ~132k |
| `mike-lessons` | lessons, memory keeper, recall, lesson delivery | ~31k |
| `mike-app` | generic API, `serve`, `cli`, TUI, setup, doctor | ~88k |
| `mike` (bin) | `main`, the migrations and the `Migrator` | ~1k |

The 18-crate map in `0002-crate-map.json` is the finer end state. Take it only
where a later measurement shows a benefit.

### Method

1. **Untangle inside the one crate.** Every cut is a module move, a type moved
   down or a call inverted, compile-checked inside today's crate. `scripts/crate-plan`
   shows the seams shrinking. No crate is created while seams remain at its boundary.
2. **Ratchet.** Each PR lowers the production seam count `crate-plan` reports, and
   none raises it. Record the count in the PR.
3. **Extract when a boundary is clean.** Move the group's directories into
   `crates/mike-<name>/src/` unchanged. The new crate keeps the same module names
   at its root, and each crate above re-exports them (`pub use mike_base::store;`), so
   `crate::store::…` paths keep resolving and only the seams change code.
4. **Measure.** After each extraction: `cargo build --timings`, the incremental
   check and test-build timings above, and update this ADR.

### Seam patterns (production references at the full target)

| Pattern | Refs | Fix |
|---|---|---|
| A. Shared API helpers live in `api` (`persist_error_response`, `persist_unavailable_response`, `change_source_from_headers`, `client_serves`, `body_search::find_in_str`, session epoch and row-id reads) | ~263 | Move them into a `web_base` module in the base. |
| B. Configuration structs and defaults owned by features (`SteererConfig`, `AnalysisConfig`, `PoolClassesConfig`, `compress::shell_output` defaults, `LlmProvider`) referenced by `config`/`settings` | ~139 | Move the plain data types and their defaults into `model`; features read them. |
| C. Pure classifiers and markers called by the capture path (`dossier::genuine` classify and markers, `compose_prompt::classify` hashing, `read_dedup` types, `store::models::ProxyEvent`/`RequestId`) | ~72 | Move them into `wire`/`core`. |
| D. Wiring placed below its users (`agents`, `scheduled_jobs`, `service_token`, `stack` probes, `settings::service`) | ~85 | Move each to the layer that owns it. |
| E. Fleet types used by launch-level code (`FleetDefinition`, `find_fleet_for_session`, session identities) | ~78 | Move the identity types and queries into `launch`, or move the users up into `fleet`. |
| F. `db` calling features (`EmbeddingService`, `memory_cap::Sheddable`, fleet state transitions) | ~44 | Move the type down, or invert behind a trait the feature implements. |
| G. Feature to feature, mostly the request pipeline calling injectors (`proxy → lessons/launch/runtime/knowledge`, `steerer → eval`, `knowledge → dossier/eval`) | ~170 | Pipeline stages register with the engine (`pipeline::RequestStage` is already the trait); shared types move down. Deferred: S5 keeps `proxy` with `dossier/steerer/eval`. |
| H. `PersistHandle` inherent methods of feature-owned groups | mechanism | One extension trait per crate (`FleetStore for PersistHandle`), re-exported through a `prelude`. Groups that stay in the base keep inherent methods. |

### Mechanisms, checked

- **Extension traits stay `Send`.** A plain `async fn` in the trait, called on the
  concrete `PersistHandle`, yields a `Send` future (compiled on 1.95: the concrete
  call passes `assert_send`; only a call generic over the handle needs
  `-> impl Future + Send`). One workspace `[lints.rust] async_fn_in_trait = "allow"`;
  `scripts/lint-rust-suppressions` rejects only structural lints. Call sites keep
  `handle.method(…)` and import the crate's `prelude`.
- **Handlers move with their feature.** 337 of about 600 `state.` accesses in `api`
  are `state.persist`; with `config` (52), the toggle overlay (33), `store` (25) and
  the settings overlay (15), most handlers need only base state. A `BaseState` in the
  base plus per-feature state extracted from `AppState` with axum's `FromRef` lets a
  feature crate export `Router<FeatureState>` and its `manifest` entries; `mike-app`
  composes them.
- **Migrations only in the binary.** The `Migrator` (`sqlx::migrate!`) moves to the
  bin crate and is passed to `serve` at start; a migration then rebuilds the bin and
  the DB test targets, not the libraries.
- **Fixtures across crates.** `#[cfg(test)]` items are invisible to other crates and
  170 files outside `store` use `store::persistence::test_support`. It moves behind
  the existing `test-seams` feature, which the crates' dev-dependencies enable.
- **Visibility.** About 11,800 `pub(crate)` items; only those that cross a boundary
  become `pub`, and the compiler names them at extraction.

### Sequence

Each row is one PR or a few; the seam counts are what each stage newly needs
(`scripts/crate-plan --stages`).

| Step | Work | Unlocks |
|---|---|---|
| 0 | `scripts/crate-plan` and the map in the repo (this ADR). Add a ratchet check to the push hook. | grading |
| 1 | Patterns A, B, C, D, F for the base boundary: 479 references. A and B are mechanical moves and most of it. | S1: extract `mike-base` and `mike-app` (81%) |
| 2 | Migrator to the bin; `test_support` behind `test-seams`. | S2 (80%) |
| 3 | `runtime → launch` (10), `launch → knowledge/steerer/lessons/eval` (16), `fleet → steerer/dossier` (3); convert the runtime, launch and fleet persistence groups to extension traits (H). | S3: extract `mike-runtime`, `mike-launch` + fleet (68%) |
| 4 | Pattern E, `launch → fleet` (78). | S4: split `mike-fleet` out (67%) |
| 5 | `knowledge → dossier/eval/lessons` (30), `steerer → lessons` (5); traits for knowledge and lessons groups. | S5: extract `mike-knowledge`, `mike-lessons` (61%) |
| 6 | Optional: pattern G (pipeline stage registration) and the rest of the 18-crate map, only if `--timings` after S5 shows a gain. | full target |

### Open decisions

- **Crate names and the S5 grouping.** `mike-features` (proxy, dossier, steerer,
  eval) is the largest crate at S5; splitting it needs pattern G first.
- **Commits in flight.** Steps 1–5 move files that other sessions edit. Land each
  step as a short-lived branch merged the same day, and announce moves in the
  session notebook so fleets do not rebase across them.
- **`async_fn_in_trait` allow.** One workspace lint setting, versus writing every
  extension method as `fn … -> impl Future + Send`.

## Consequences

- Progress is a number that only goes down, and work can stop at any stage with a
  working build: S1 already separates the base and the app layer.
- Steps 1–2 change no behaviour; they move types and modules. Steps 3–5 change how
  persistence methods are declared, not what they run.
- DB-tier tests (`make test-db`) are required for every step that moves
  persistence code.
- `.config/test-tiers.json` and `.config/nextest.toml` are keyed by module and
  binary names; they move with the crates at each extraction.
