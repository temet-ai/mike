---
title: One owning crate per Postgres schema, with core tables in the base
status: proposed
created: 2026-11-12
updated: 2026-11-12
summary: The database already has 26 schemas, but `memory` (83 tables) belongs to six crates and 54 tables are written by a crate that does not own them. Each schema, or table where it differs, gets one owning crate (docs/adr/0002-crate-map.json, schemas.owners); only the owner writes, the base owns a stable core, `memory` and `launch` split by owner with ALTER TABLE … SET SCHEMA, and the feature-toggle key CHECK that costs a migration per toggle goes. scripts/schema-plan grades it.
---

# ADR 0003 — One owning crate per Postgres schema

**Status: proposed.** Measured on `main` at `9c8c7bab` (12 November 2026).
Companion to [ADR 0002](0002-split-crate-mike.md); the ownership map is the
`schemas` section of [`0002-crate-map.json`](0002-crate-map.json) and
`scripts/schema-plan` grades the SQL against it.

## Context

| Measure | Value |
|---|---|
| Schemas, tables | 26 schemas, 253 tables (`memory` alone 83) |
| SQL references | 6,814 schema-qualified table references outside migrations |
| Reads of a table another crate owns (S5 crates) | 1,489 of 3,480 production reads (43%) |
| Tables written by more than one crate | 43 at S5; 54 against the ownership map below (146 references) |
| Migrations since the baseline | 354; `memory` 86, `config` 39, `steering` 37, `requests` 26 |
| Of which widen the toggle key CHECK | 65 (`config.feature_toggles`, one per new toggle) |
| Migrations since 20 Oct touching the core schemas | `requests` 5, `sessions` 3, `projects` 2, `tools` 1, `responses` 0, `agents` 0 — against `memory` 32 |

What this means for the crate split:

1. **SQL is not a compile dependency.** A query in one crate reading another
   crate's table creates no Rust dependency, and sqlx's offline `.sqlx` data never
   consults the database at compile time. Schemas therefore do not shrink the build
   graph by themselves.
2. **They decide where persistence code lives.** A group of queries belongs in the
   crate that owns the tables it writes. Today the schemas mostly say that already:
   `runtime`, `steering`, `eval`, `work`, `wright` each have one writer. `memory`
   and `launch` do not.
3. **A lower crate may read a higher crate's data through SQL** without the upward
   Rust reference ADR 0002 would have to cut. A contract view makes that read safe
   against the owner's migrations.
4. **The core is real and quiet.** The capture path in the base writes `requests`,
   `sessions`, `projects`, `tools`, `responses`, `agents`, `messages` and the turn
   tables, every crate reads them, and they change far less than `memory`.

## Decision

### D1. One owner per schema or table

`schemas.owners` names the owning crate of each schema, or of a table where it
differs (longest key wins). Only the owner writes. Another crate changes the data
by calling the owner's Rust API, which is a downward call when the owner is lower
(every write to a core table) and a seam ADR 0002 must invert when it is not.

| Owner | Schemas and tables |
|---|---|
| base (`db`) | `requests`, `sessions`, `projects`, `tools`, `responses`, `agents`, `messages`, `system_prompts`, `prompt_kinds`, `llm`, `config`, `claude_proxy`, `statements`, `workspace` (registrations, coordination), and the turn tables `memory.operator_turns`, `operator_turn_chunks`, `operator_turn_legacy_labels`, `turn_outcomes`, `lesson_file_receipts` |
| runtime | `runtime`, `mailbox.runtime_notifications` |
| launch | `launch` (launches, templates, backend validations), `mailbox`, `work`, `workspace.questions`, `workspace.session_notes`, `memory.change_generations`, `memory.seat_memory_items` |
| fleet | `launch.fleets`, `fleet_events`, `introductions`, `run_records`, `runs`, `told`, `tree_changes`, `tree_checks`, `workspace.change_events`, `change_places`, `memory.carried_lesson*` |
| knowledge | the 22 `memory` tables of knowledge, semantic, search, embeddings and patterns |
| dossier | `memory.turn_facts`, `turn_facts_backstop_attempts`, `dossier_entries`, `ask_ledger_entries` |
| steerer | `steering`, `memory.acceptance_criteria*`, `memory.directive_proposal*` |
| eval | `eval`, `session_judges`, `memory.turn_verdicts` |
| proxy | `overrides`, `rules`, `remote_control` |
| lessons | the rest of `memory`: lessons, lesson relations and placements, skills, consolidation, supplied items |
| app | `wright`, `jobs`, `sync` |

### D2. Split `memory` and `launch` by owner

Move each owner's tables into its schema with `ALTER TABLE … SET SCHEMA`. Postgres
changes only the catalog: indexes, constraints, foreign keys, owned sequences and
grants move with the table, and the statement does not rewrite data. Functions,
triggers and views that name a moved table must be recreated in the same
migration. The lessons tables (the largest group) stay in `memory`, so 39 tables
move instead of 83:

| To | Tables |
|---|---|
| `knowledge` (new) | 22 from `memory` |
| `dossier` (new) | 4 from `memory` |
| `turns` (new, base) | 5 from `memory` |
| `steering` | 4 from `memory` |
| `eval` | `memory.turn_verdicts` |
| `fleet` (new) | 3 from `memory`, 8 from `launch`, 2 from `workspace` |
| `work` | 2 from `memory` |

One migration per destination, landed with the commit that rewrites the qualified
names in the SQL (a scripted replace) and refreshes `.sqlx` with
`scripts/sqlx-prepare`. Each move lands with the ADR 0002 step that extracts its
crate: `fleet` with S3–S4, `knowledge` and `dossier` with S5.

### D3. Contract views only where a table churns

477 production reads touch a feature table another crate owns (74 tables). Views
are the contract for those reads, but not wholesale: sqlx infers a column's
nullability from `pg_attribute.attnotnull`, which is false for every view column, so
a query moved onto a view may turn its fields into `Option` and need `"col!"`
overrides. Introduce a view where the owner's table actually changes and has
foreign readers. First candidates, by foreign reads and alterations:
`memory.turn_facts` (51 foreign reads, 13 alterations), `memory.lessons` (19,
12), `launch.launches` (23). Pilot one view first and record what sqlx does with it.

### D4. Drop the toggle key CHECK — done

Done in migration `20261112160000_config_feature_toggles_open_keys.sql`.

`config.feature_toggles.key` carries a closed CHECK constraint that a migration
widens for every new toggle: 65 of 354 migrations. `FeatureToggleService::set_with_source`
already rejects any key not in `TOGGLE_META` before it writes, so the constraint
duplicates the code. Drop it in one migration; new toggles then need no migration.
This is independent of the crate split and can land first.

### D5. One schema per migration

Name migrations `<timestamp>_<schema>_<what>.sql` and keep each to one schema (D2's
moves excepted), so a crate's migrations are identifiable and review knows the
owner. The migrations stay in one directory with one ledger, held by the binary
(ADR 0002); sqlx's `migrate!` embeds a single directory.

## Grading

`scripts/schema-plan` reports foreign writes (146 references, 54 tables today; the
target is zero), foreign reads of feature tables (477 references, 74 tables; the
contract-view candidates) and migrations per schema. Like `crate-plan`, the counts
only go down.

## Consequences

- Writes to core tables from features become calls into the base's persistence,
  which ADR 0002 places below every feature: no new seam.
- D2 renames tables that dashboards, ad-hoc SQL and `mike-mcp` read tools may
  name; `rg` the qualified names across the repository before each move.
- D3 can turn query fields into `Option`; the pilot decides whether views or the
  owner's Rust API are the default contract.
- D4 removes the database's own check on toggle keys; the service check and its
  tests become the only guard.
