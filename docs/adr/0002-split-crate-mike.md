---
title: Split crate mike into peer crates along measured seams
status: accepted
created: 2026-11-12
updated: 2026-11-14
summary: Crate mike (517k lines) is one compilation unit and 99% of it one dependency cycle. A layered split saves little because persistence, at the bottom, changes in 45% of commits; the split that pays makes launch, fleet, runtime, knowledge and lessons peer crates over a thin base. The untangling happens inside the one crate first, graded by scripts/crate-plan against a committed target map, and a group becomes a crate only when nothing below it reaches up into it. Stage S5 (9 crates, 705 references cut) replays history at 61% of today's recompiled lines by references; S6 puts fleet beside knowledge, features and lessons, and the replay by Cargo dependencies is 73% (the chain was 75%).
session_id:
  - 6bf6170d-77c4-477e-9e50-5b6dda36334e
---

# ADR 0002 — Split crate mike into peer crates along measured seams

**Status: accepted (12 November 2026):** the 9-crate S5 target, no in-flight coordination, persistence by repository accessors. Measured on `main` at `9c8c7bab` (12 November 2026, after
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
| H. `PersistHandle` inherent methods of feature-owned groups | mechanism | Each feature crate owns a repository type (`FleetStore<'a>` holding `&PersistHandle`) with the group's methods as ordinary inherent `async fn`s, reached through one accessor: `persist.fleet().complete_run(…)`. Groups that stay in the base keep inherent methods on `PersistHandle`. |

### Mechanisms, checked

- **Repository accessors, no lint allowance.** A feature crate cannot add inherent
  methods to `PersistHandle`, and an `async fn` in a public trait needs the
  `async_fn_in_trait` lint allowed, which would weaken the lint policy this work is
  meant to tighten. Instead each feature crate defines a repository type borrowing
  the handle (`pub struct FleetStore<'a> { db: &'a PersistHandle }`) with inherent
  `async fn`s, and one accessor trait without async methods
  (`trait FleetPersist { fn fleet(&self) -> FleetStore<'_>; }`). Compiled on 1.95
  under `#![deny(warnings)]`: the futures are `Send`, including inside a spawned
  task. Call sites change from `persist.complete_fleet_run(…)` to
  `persist.fleet().complete_fleet_run(…)`; `scripts/crate-plan` already maps every
  method to its crate, so the rewrite is scripted.
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
| 0 | Done: `scripts/crate-plan`, `scripts/schema-plan` and the map in the repo; the push hook runs `--check` on both against `ratchet` in the map (874 seams, 146 foreign writes on 12 Nov). | grading |
| 1 | Done (13 Nov): patterns A, B, C, D, F for the base boundary, the repository accessors (H) for every group above `db`, and the S1 extraction: `crates/mike-base`, `crates/mike-features`, and `crates/mike` as the app and binary. | S1 (81%) |
| 2 | Migrator to the bin; `test_support` behind `test-seams`. | S2 (80%) |
| 3 | Done (13 Nov): `BaseState` in `mike-base` (`AppState` derefs to it; handlers extract it or `LaunchState` through `FromRef`), `MailPush` for remote-control pushes, shared reads down to `mike-base`, Codex home helpers into `runtime`; then the S3 extraction: `crates/mike-runtime` and `crates/mike-launch` (launch and fleet). | S3 (68%) |
| 4 | Pattern E, `launch → fleet` (78). | S4: split `mike-fleet` out (67%) |
| 5 | `knowledge → dossier/eval/lessons` (30), `steerer → lessons` (5); repository accessors for the knowledge and lessons groups. | S5: extract `mike-knowledge`, `mike-lessons` (61%) |
| 6 | Optional: pattern G (pipeline stage registration) and the rest of the 18-crate map, only if `--timings` after S5 shows a gain. | full target |

### Decisions taken

- **Target:** S5, 9 crates. The 18-crate map stays a reference for later splits.
- **No coordination with other sessions:** nothing else runs on mike while the
  split is in progress, so steps land as ordinary branches.
- **No `async_fn_in_trait` allowance:** persistence moves through repository
  accessors (pattern H).

### S1 as built

- **Crates.** `mike-base` (core, wire, model, db, llm), `mike-features` (everything
  between them and the app, until S3 splits it), `mike` (web, serve, cli, bin). Each
  keeps its files at the module paths they had.
- **Facades.** A module a crate needs only because it owns something below it is a
  facade: `pub use <lower crate>::<path>::*;` plus the children it owns (files that
  begin `//! Facade:`). A crate's own module file gains the same glob when a lower
  crate also has that module, so `crate::store::persistence::X` resolves in every crate.
- **Visibility.** The lower crates use `pub` where the single crate used `pub(crate)`,
  `pub(super)` or `pub(in …)`; test modules keep their original visibility.
- **Test seams.** Test helpers other crates' tests use are gated
  `#[cfg(any(test, feature = "test-seams"))]`; each crate's dev-dependencies enable
  `test-seams` on the crates below it. The live-install guard follows the same gate,
  so it stays armed in every crate's tests.
- **Tests that reach a higher crate** live in that crate's `cross_crate_tests`
  module (`crates/mike/src/cross_crate_tests/`, `crates/mike/src/api/cross_crate_tests/`,
  …), never under a fast-tier module.
- **Graders.** `crate-plan` and `schema-plan` read all three `src/` trees; the S1 stage
  reads 0 seams, the full target 593.

Measured on 13 Nov (cloud container, `-j 4`, warm cache, one file touched):

| Touched crate | `cargo check -p mike --lib` | that crate's lib test build | before (one crate) |
|---|---|---|---|
| `mike` | 31 s | 24 s | check 35 s, test build 114 s |
| `mike-features` | 26 s | 45 s | |
| `mike-base` | 30 s | 51 s | |

The test build is where the split pays: an edit rebuilds only its own crate's test
harness. `check` gains little because incremental compilation already skipped
unchanged code.

### S5 as built

- **Crates.** `mike-knowledge` (project knowledge, semantic, rerank, embeddings worker,
  search events) sits on `mike-launch` and under `mike-features`; `mike-lessons`
  (lessons, memory keeper, recall, lesson delivery, supply verdicts, lesson pairs) sits
  on `mike-features` and under `mike`. Root globs chain `mike_base` ← `mike_runtime` ←
  `mike_launch` ← `mike_knowledge` ← `mike_features` ← `mike_lessons` ← `mike`.
- **Knowledge state.** Knowledge takes `KnowledgeState` (BaseState plus the semantic,
  embedding and rerank services) through `FromRef`. Drift alarms and rerank tuning
  write through `DriftAlarms` and `SettingWriter`, which eval and settings implement.
- **Lessons above the proxy.** The snapshot types live in `mike_base::lesson_cache`.
  The proxy reaches lessons only through `LessonHooks` on `AppState`: the four lesson
  pipeline stages, the harvester, supply records, attribution, the proposer tick and
  the rerank lesson-pairs phase. The hooks do nothing until `serve` installs
  `lesson_delivery::hooks::Lessons`; a test that wants lessons injected installs them too.
- **Graders.** S5 incremental seams 0 production, 0 test; the remaining S5-stage 76
  are S4's launch → fleet.

Measured on 13 Nov on omit (mbx, half the cores, `nice`; one blank line added to the
crate's `lib.rs`, one run each):

| Touched crate | that crate's lib test build | `cargo check -p mike --lib` |
|---|---|---|
| `mike-knowledge` | 42 s | 70 s |
| `mike-lessons` | not measured: the run waited on another session's build | 44 s |

### S4 as built

- **Crate.** `mike-fleet` (`launch/fleet`, change record, questions, their persistence
  and API handlers) sits on `mike-launch` and under `mike-knowledge`. Root globs chain
  `mike_base` ← `mike_runtime` ← `mike_launch` ← `mike_fleet` ← `mike_knowledge` ←
  `mike_features` ← `mike_lessons` ← `mike` (#235, #236).
- **Launch reaches fleet only through `SeatLaunchHook`.** `mike` installs it in
  `main.rs` and in `AppState` construction; any other entry point that opens fleet seats
  calls `mike::launch::cli::fleet_settings::install()`, or fleet runs are refused with
  "no fleet support installed".
- **Graders.** Every stage's seams are 0 production, 0 test; `crate_seams` 348 and
  `foreign_writes` 105 are the ratchets left for later work.

Measured on 14 Nov on omit at `9e416ee43` (mbx, half the cores, `nice`, every unit warm;
one blank line added to the crate's `lib.rs`, one run each; a warm workspace lib test
build is 3 m 40 s):

| Touched crate | that crate's lib test build | `cargo check -p mike --lib` |
|---|---|---|
| `mike-base` | 25 s | 65 s |
| `mike-runtime` | 18 s | 57 s |
| `mike-launch` | 17 s | 47 s |
| `mike-fleet` | 40 s | 40 s |
| `mike-knowledge` | 29 s | 29 s |
| `mike-features` | 51 s | 36 s |
| `mike-lessons` | 46 s | 18 s |
| `mike` | 108 s | 12 s |

An edit below `mike` now builds its own tests in 17–51 s, against 114 s for the single
crate. Crate `mike`'s own test build is still 108 s: its API, serve and CLI tests and
the `cross_crate_tests` modules all link there.

### S6 as built

- **Side by side, not chained.** A chain of crates still rebuilds everything above an
  edit, so S5's chain kept most of the single crate's cost. The Cargo graph is now
  `mike-runtime` → `mike-base`, `mike-launch` → `mike-runtime`, `mike-fleet` and
  `mike-knowledge` → `mike-launch`, `mike-features` → `mike-knowledge`, `mike-lessons` →
  `mike-features`, and `mike` → `mike-fleet` + `mike-lessons`. An edit in fleet no
  longer rebuilds knowledge, features or lessons, and an edit in knowledge, features or
  lessons no longer waits on fleet. The map's `package_deps` records these edges.
- **Root globs follow the graph.** `mike_knowledge` globs `mike_launch`; `mike` globs
  both `mike_lessons` and `mike_fleet`, and names the modules that both sides extend
  (`launch`, `api::workspace`, `store::persistence::{change_record, mailbox, questions,
  session_notes, test_support, workspace}`, `analysis`, `dossier`, `extract`,
  `memory_cap`, `observability`, `settings`, `workspace`) explicitly, so a glob
  ambiguity cannot arise.
- **What fleet supplied now plugs in through launch.** `fleet_settings::install()`,
  which `mike` calls in `main.rs` and in tests that need fleet behaviour, installs three
  hooks: `SeatLaunchHook`, `launch::session_tail` (the change-record lines since a
  session last looked, read by the reminders stage and client context) and
  `session_notes::NoteRules` (acceptance-obligation checks, carry and ruling notices).
  Session-note writes live in `LaunchPersist`; `FleetPersist`'s note writes pass fleet's
  rules explicitly. Without `install()` a note is written with no rules and the tail is
  empty. AppState construction no longer installs anything.
- **Tests that need fleet moved to `crates/mike/src/cross_crate_tests`**: launch
  brief delivery, fleet admission, fleet run notes, questions, fleet carry and the
  change-record tail stage.
- **The grader tells the truth.** `scripts/crate-plan` replays a map that has
  `packages` by the packages' Cargo dependencies (a touched crate rebuilds its whole
  package and every package depending on it), and bills a migration only to the bin.
  Before, the replay followed module references, so it charged 80% for the chain and
  for the side-by-side graph alike. `--check` also fails when a package's
  `Cargo.toml` names a split package that `package_deps` does not put below it.

Replay of 232 commits by Cargo dependencies: the S5 chain 75%, side by side 73%.

### S7 pattern: a feature crate owns its settings

Before S7, each feature's settings sat in `mike-base`: a `settings/meta_<f>.rs` listed in
`SETTINGS_META`, a typed field per section on `ProxyConfig`, arms in
`config/overrides*.rs` and `settings/overlay_apply*.rs`, and fields on `SettingsOverlay`
and `ResolvedToggles`. Adding one setting rebuilt every crate. S7-0 added the
registration machinery and moved the lessons settings as the pilot
(`crates/mike-lessons/src/lessons_settings/`). The other crates move the same way.

**What base keeps.** The generic machinery stays in base:
- `SettingMeta`, validation and the settings service;
- `ConfigSections` (`config/sections.rs`), the untyped section map that `ProxyConfig`
  flattens;
- `FeatureOverlays` (`settings/feature_overlays.rs`), the typed per-feature slots on
  `SettingsOverlay` and `ResolvedToggles`;
- the registry (`settings/registry.rs`).

Global settings stay in base too, along with every setting base code reads itself.
`SETTINGS_META` keeps its name and is base's entries followed by each registered
feature's, in registration order.

**What a feature crate adds** (one module, `<crate>/src/<feature>_settings/`):

1. `sections.rs`: the typed config structs, moved unchanged with their `Default`, and
   `impl ConfigSection for X { const KEY: &'static str = "<toml table name>"; }`. The key
   is the old `ProxyConfig` field name, so `[table]` in config.toml, `MIKE_<TABLE>__<FIELD>`
   and `print-config` keep working. Only nested sections can move: a flattened group of
   top-level keys stays on `ProxyConfig`.
2. `meta.rs`: `pub fn entries() -> Vec<SettingMeta>`, the entries cut out of base's
   `meta_*.rs`, with key, group, default and text unchanged.
3. `overrides.rs`: `apply_override(&mut ConfigSections, key, value) -> bool`, the
   `apply_settings_overrides` arms rewritten as
   `sections.update::<X>(|cfg| …)`. Return `false` for any key you don't own. Also
   `boot_toggles(&ConfigSections) -> Vec<(&'static str, bool)>`, the toggle names and
   values that `build_feature_toggle_overlay` used to read from your sections
   (`("x", sections.get::<XConfig>().enabled)`); it applies them by name, so the
   registry never names `FeatureToggles`.
4. `overlay.rs` (when the feature has live, `NextMessage`/`NewSessions` settings): one
   `#[derive(Debug, Clone, PartialEq)] struct` with a `Default` holding the old
   `SettingsOverlay::default()` values, `impl OverlaySection`, and
   `apply_overlay(&mut FeatureOverlays, key, value) -> bool` built on the `set_*`
   helpers in `settings::overlay_apply`.
5. `mod.rs`: `pub static <F>_SETTINGS: FeatureSettings = FeatureSettings { name, meta,
   sections: &[SectionSpec::of::<X>(), …], apply_override, apply_overlay, boot_toggles }`
   and `pub fn register() { crate::settings::registry::register(&<F>_SETTINGS); }`.
   A feature with no overlay or no boot toggles passes a function that returns `false`
   or does nothing.
6. Add one line to `crates/mike/src/feature_settings.rs`'s `install()`.
   `main.rs` calls it before `Cli::parse`.

**Readers change mechanically:**

| Before | After |
|---|---|
| `config.lesson_worker.x` | `config.sections.get::<LessonWorkerConfig>().x` (returns a clone; bind it once per function) |
| `config.lesson_relations.enabled = true` (tests) | `config.sections.update::<LessonRelationsConfig>(\|c\| c.enabled = true)` |
| `overlay.recall_injector_max_lessons_per_topic` | `overlay.features.get::<RecallOverlay>().max_lessons_per_topic` |
| `toggles.recall_injector_max_lessons_per_topic` (`ResolvedToggles`) | `toggles.features.get::<RecallOverlay>().max_lessons_per_topic` |
| `crate::settings::meta_<f>::KEY` | `crate::<feature>_settings::meta_<f>::KEY` |

A reader below the feature crate can't name the feature's type. Either the reader moves up,
or the feature passes it what it needs through a hook it already has. For example,
`LessonHooks::response_hook(&ProxyConfig, Option<bool>)` lets lessons read its own
`HarvesterConfig`. No crate adds a `mike-*` dependency to reach a section.

**Rules that keep behaviour the same.**
- Every key, default, label, group and description moves byte for byte. `/settings`,
  validation and `print-config` read the merged registry.
- Precedence is unchanged. `ProxyConfig::default()` holds each registered section's
  default, and figment merges the file, `MIKE_*` and CLI over it. An unknown key inside a
  registered section, or a wrong type there, still fails validation, because each
  section parses with its own `deny_unknown_fields` struct.
- Registration must happen before the first `ProxyConfig::default()`. In the binary,
  `main.rs` does it. An in-process test that loads a TOML naming a feature's section, or
  writes a feature's key through the settings service, calls
  `mike::feature_settings::install()` (or the feature's `register()` inside its own
  crate) first. Without registration a process sees no feature sections: their TOML
  tables are ignored, their keys are unknown to `/settings`, and readers get the
  compiled defaults.
- `register` is idempotent and keyed by `name`. Tests may call it as often as they like.

**What it bought.** The lessons pilot removed 4 config files, `meta_memory_loop.rs`, 2
override functions, 1 overlay arm and 2 meta functions from base. An edit to a lessons
setting now rebuilds only `mike-lessons` and `mike`.

## Consequences

- Progress is a number that only goes down, and work can stop at any stage with a
  working build: S1 already separates the base and the app layer.
- Steps 1–2 change no behaviour; they move types and modules. Steps 3–5 change how
  persistence methods are declared, not what they run.
- DB-tier tests (`make test-db`) are required for every step that moves
  persistence code.
- `.config/test-tiers.json` and `.config/nextest.toml` are keyed by module and
  binary names; they move with the crates at each extraction.
