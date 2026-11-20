---
thread: central-settings-page
status: parked
last_session: 2026-06-16 (session 23214930-40a5-41bd-b773-3d13e30b10b8) @ 75b2c5a7
updated: 2026-09-03
---

## State of play
Where we are: Operator confirmed scope = **Everything (A+B+C)** — `/settings` becomes the full
system-configuration surface for every operator-tunable field in `ProxyConfig`, not just the 6 binary
feature toggles. Stage 1 (migration) DONE. Stage 2 backend is **half-landed**: a single mega-dispatch
crashed (API Overloaded) at 63 tool calls; rather than re-run it, the work is now decomposed into small
compile-gated chunks in **IMPLEMENTATION_PLAN.md** (the resume state-machine). The existing 6-toggle
mechanism is the pattern being generalized, NOT replaced.

Stage 2 done/missing (evidence-grounded):
- DONE: `settings/mod.rs` (40-entry SETTINGS_META + enums + SettingsOverlay + validate),
  `settings/service.rs` (SettingsService + tests), `store/persistence/settings.rs` (fetch/upsert + tests),
  module decls (lib.rs, persistence/mod.rs), AppState fields (settings_overlay, settings_service,
  SettingsWriteGuard).
- MISSING: service never instantiated in AppState::new; no ProxyConfig fold; ResolvedToggles not extended;
  no read-site rewiring; no GET/PATCH /api/settings catalogue; no routes; no manifest entries.

Next (each chunk ends GREEN before the next — see IMPLEMENTATION_PLAN.md):
(task 14) 2f+2g — wire SettingsService into AppState::new + apply_settings_overrides fold onto ProxyConfig.
(task 18) 2h+2i — extend ResolvedToggles (in feature_toggles/mod.rs!) + rewire reminders/weasel/recall read-sites.
(task 19) 2j+2k+2l — GET /api/settings catalogue + PATCH + routes + manifest.
(task 20) 2m — .sqlx regen + full make check.
(task 15) Stage 3 frontend; (16) Stage 4 story; (17) Stage 5 docs.

Scope correction discovered: `ResolvedToggles` + `from_overlay` live in `feature_toggles/mod.rs`, NOT
`config/mod.rs` as design.md's allowlist assumed. IMPLEMENTATION_PLAN 2h captures this.

Files:
- docs/threads/central-settings-page/IMPLEMENTATION_PLAN.md — THE resume state-machine: per-unit checklist + verification gates
- docs/threads/central-settings-page/design.md — the build contract: catalogue, reactivity table, file allowlist, staging
- crates/claude-proxy/src/feature_toggles/{mod,service}.rs — the pattern being generalized
- crates/claude-proxy/src/store/persistence/feature_toggles.rs — DB pattern to mirror
- crates/claude-proxy/src/config/mod.rs — ProxyConfig + AppState + ResolvedToggles + SessionToggleCache
- crates/claude-proxy/src/api/handlers.rs (~8794) — list_features / patch_feature REST
- frontend/src/routes/settings/+page.svelte — current 6-toggle page
- frontend/src/routes/settings.stories.svelte — design spec to update

Open questions: none (scope confirmed Everything; per-field reactivity derived from operator's stated rule).
Decisions:
- Keep `config.feature_toggles` (6-key closed CHECK) as-is; add parallel `config.settings` (open, JSONB,
  Rust-side META validation) for the expanded surface. Unify only at the REST catalogue + frontend layer.
- Reactivity per field follows WHERE the value is read: boot/spawn readers → apply-on-restart (DB override
  folded onto ProxyConfig at startup, no overlay needed); per-turn hot-path readers → session-frozen via
  extended ResolvedToggles (applies to new sessions, never re-eval per turn).
- Analysis config = OnRestart (AnalyzerRegistry is baked at startup; no live registry-rebuild — avoids scope creep).
- EXCLUDE from UI: `steerer.openrouter_api_key` (secret), `steerer.force_fire` (TEST/DEBUG carve-out).
Blockers: none.

## Session log
- 2026-06-16 (23214930) @ 75b2c5a7 — Thread created. Mapped the 6-toggle mechanism end to end
  (feature_toggles mod/service, persistence, AppState wiring, session freeze, REST). Extracted exact
  tunable fields/types/defaults from 7 sub-config structs (steerer, reminders, weasel, embedding,
  cap_manager, harvester, recall_injector) via Explore agent. Flagged secret (openrouter_api_key) +
  debug knob (force_fire) for exclusion. Wrote design.md contract. Began Stage 1 (db-dev migration).
- 2026-06-16 (23214930) — Stage 1 DONE: db-dev created `20260616000002_config_settings.sql` (open-keyspace
  config.settings, no seed, no CHECK). Stage 2 dispatched as one big rust-developer task → crashed (API
  Overloaded) at 63 tool calls with backend half-wired. Operator: "create a dedicated plan… otherwise it
  will be half implemented." Stopped the resume, swept on-disk state, wrote IMPLEMENTATION_PLAN.md
  (per-unit compile-gated checklist), decomposed Stage 2 into 4 green-gated chunks (tasks 14/18/19/20 with
  blockedBy chain). Discovered ResolvedToggles lives in feature_toggles/mod.rs (allowlist correction).
  Next: execute task 14 (2f+2g).
- 2026-09-03 (thread sweep) — status active -> parked: no activity since 2026-06-16
