---
title: Central Settings Page — Granular Implementation Plan
status: active
created: 2026-06-16
updated: 2026-06-16
summary: Unit-by-unit build plan for the central /settings backend + frontend, with a verification gate per unit so the work resumes deterministically across crashes and never lands half-implemented. Stage 5 (docs) complete.
session_id:
  - 23214930-40a5-41bd-b773-3d13e30b10b8
---

# Central Settings Page — Granular Implementation Plan

This is the resumable checklist for the work scoped in [`design.md`](design.md)
(operator decision: `/settings` becomes the full operator-tunable `ProxyConfig`
surface — "Everything (A+B+C)"). `design.md` is the *contract* (what + why);
this file is the *state machine* (what is done, what remains, in what order,
verified how). Every unit is small enough to land + compile on its own. Tick a
box only when its verification gate passed with captured output.

## Rule: every unit ends green

The single mega-dispatch crashed at 63 tool calls and left Stage 2 half-wired.
The fix is granularity: **no unit is "done" until `cargo check -p claude-proxy`
(or the named scoped test) ran clean and the output was captured.** A unit that
compiles is a safe resume point; a unit that doesn't is not started.

---

## Stage 2 — backend (rust-developer): current state (evidence-grounded 2026-06-16)

Verified by source sweep of the crashed agent's on-disk output.

### DONE — landed, structurally complete (still gated on a full compile)

- [x] **2a. `settings/mod.rs`** — `SettingKind` / `SettingScope` / `SettingApplies`
  enums, `SettingMeta` struct, `SettingsOverlay` + `Default`,
  `apply_db_rows_to_overlay`, `validate_setting_value`, and `SETTINGS_META`
  (**40 entries**). Excludes the 3 forbidden keys (secret/force_fire/sweep).
  ✅ **Compile fix landed (task #14):** `SETTINGS_META` → `LazyLock<Vec<SettingMeta>>`
  (resolved 24× E0015) and dropped `Deserialize` from `SettingMeta`
  (now `#[derive(Debug, Clone, Serialize)]`; resolved 2× E0277). Verified clean.
- [x] **2b. `settings/service.rs`** — `SettingsServiceError`, `SettingsService`
  with `reconcile_on_startup` / `set` / `fetch_all` + 4 unit tests.
- [x] **2c. `store/persistence/settings.rs`** — `fetch_settings` / `upsert_setting`
  (+ 5 tests). SQL confined to `PersistHandle` methods.
- [x] **2d. module decls** — `lib.rs:109 pub(crate) mod settings;`,
  `store/persistence/mod.rs:22 pub(crate) mod settings;`.
- [x] **2e. `AppState` fields** — `settings_overlay: Arc<ArcSwap<SettingsOverlay>>`,
  `settings_service: Option<Arc<SettingsService>>`, `SettingsWriteGuard` marker.

### MISSING — the half that wasn't reached (the actual remaining work)

- [x] **2f. Wire `SettingsService` into `AppState::new`.** ✅ Landed (task #14):
  `SettingsService::reconcile_on_startup` at `config/mod.rs:1165`, mirroring the
  `FeatureToggleService` block; `settings_service` field populated. Verified clean.

- [x] **2g. Fold DB overrides onto `ProxyConfig` at startup (OnRestart fields).**
  ✅ Landed (task #14): `fn apply_settings_overrides(cfg: &mut ProxyConfig, rows:
  &[(String, Value)])` at `config/mod.rs:1357`, called at `config/mod.rs:1011`
  (mutates owned `ProxyConfig` before Arc-wrap, after `fetch_settings`). Folds
  OnRestart keys; skips unknown/session-scoped keys silently. Unit test
  `apply_settings_overrides_folds_int_bool_enum_keys` (line 1958) passed.

- [x] **2h. Extend `ResolvedToggles` with session-scoped params.** ✅ Landed
  (task #18): `ResolvedToggles` (feature_toggles/mod.rs) now carries the 3 bools +
  6 params; `from_overlay(overlay: &FeatureToggles, settings: &SettingsOverlay)`;
  `resolve_session_toggles` (config/mod.rs:847) loads BOTH overlays inside
  `or_insert_with` (frozen once per session). Tests updated. Verified clean.

- [x] **2i. Rewire per-turn read-sites to the frozen params.** ✅ Landed (task #18):
  all 3 sites in `proxy/handler.rs` now read the 6 params from frozen
  `session_toggles` (recall `max_lessons_per_topic`:460; reminders
  `inject_lessons`/`max_lessons_to_inject`:501-502; weasel
  `forbidden_words`/`correction_prompt`/`max_buffer_bytes`:1179/1191/1247). No
  live overlay reads on the hot path. The actual reads live in handler.rs, NOT
  reminders/mod.rs or recall_injector/mod.rs (plan's allowlist over-listed those).
  Gate: `cargo check -p claude-proxy` + `--tests` 0 errors; 2 session-toggle tests passed.

- [x] **2j. REST: `GET /api/settings` + `PATCH /api/settings/{key}`.** ✅ Landed
  (task #19): `list_settings` (full SETTINGS_META + effective value from
  `fetch_all()` else `meta.default`; top-level `steerer_openrouter_configured_via_env`
  bool via `Option<SecretString>::is_some()` — value never exposed) and
  `patch_setting` (body `{value}`; 503/404/400/500 mapping) in api/handlers.rs.
  `/api/settings/features` unchanged.

- [x] **2k. Route registration.** ✅ Landed (task #19): `GET /api/settings` +
  `PATCH /api/settings/{key}` in api/mod.rs:931-932. `features` literal segment
  matches before `{key}` capture — confirmed no route conflict at build.

- [x] **2l. Manifest entries.** ✅ Landed (task #19): both new routes added to
  api/manifest.rs:629/636 as `Exempt`. `cargo nextest run -p proxy-mcp` — 86 passed,
  0 failed (both parity directions green). fidelity-check CLEAN.

- [ ] **2m. `.sqlx` + full gate.** Regenerate `.sqlx` from repo root
  (`cargo sqlx prepare --workspace`, DATABASE_URL from `.env`, never
  `SQLX_OFFLINE`). Then `make check` ONCE. *Gate:* `make check` failure-count 0,
  output captured.

**Allowlist for the remaining backend units (2f–2m):** `config/mod.rs`,
`feature_toggles/mod.rs` (2h only — the documented scope correction),
`api/handlers.rs`, `api/mod.rs`, `api/manifest.rs`, `reminders/mod.rs`,
`recall_injector/mod.rs`, the weasel read-site in `proxy/handler.rs`, and `.sqlx/`.
Nothing else. If a unit needs a file outside this list, STOP and report.

---

## Stage 3 — frontend (frontend-developer)

Depends on 2j's REST shape being final. Generic, metadata-driven render — no
per-field hardcoding, no truncation (inspection-fidelity).

- [ ] **3a. `lib/api/settings.ts`** — catalogue DTO mirroring the GET shape +
  `getSettings()` / `patchSetting(key, value)`.
- [ ] **3b. per-kind controls** under `lib/components/settings/` — toggle / number
  / text / string-list / enum + a group section with scope + applies badges,
  label, description. Full values, scroll not clip.
- [ ] **3c. `routes/settings/+page.svelte`** — group `SETTINGS_META` by `group`,
  render controls generically, PATCH on change, surface applies="on_restart"
  vs "new_sessions" honestly.
- *Gate:* `npm run build` clean + load `/settings` via Chrome MCP, screenshot or
  `read_page` excerpt showing the grouped surface.

## Stage 4 — story (ui-designer)

- [ ] **4. Respec `routes/settings.stories.svelte`** — remove the "all six
  toggles, nowhere else" assertion; document the full grouped surface. *Gate:*
  `npm run storybook` renders.

## Stage 5 — docs (technical-writer)

- [x] **5a. `/settings`** header — `/settings` is the full config surface.
  ✅ Landed (2026-06-16): replaced the "feature toggles only" paragraph with a
  two-section description covering master toggles AND the 40-entry catalogue;
  added REST endpoint shapes, the `steerer_openrouter_configured_via_env` note,
  and the session-freeze vs on-restart reactivity distinction.
- [x] **5b. `docs/ARCHITECTURE.md`** — settings-subsystem paragraph (config.settings
  open-keyspace table, OnRestart fold vs session-freeze split).
  ✅ Landed (2026-06-16): added "Live settings subsystem" paragraph to §7
  covering both REST endpoints, SETTINGS_META (40 entries), SettingsOverlay
  ArcSwap + resolve_session_toggles, apply_settings_overrides at startup, and
  the disjoint-endpoint design rationale.

---

## Dependency order

2f → 2g → 2h → 2i → 2j → 2k → 2l → 2m (backend, each green before next) →
3a → 3b → 3c → 4 → 5. 2g and 2h are independent of each other (can interleave);
both precede 2i. 4 can start once 2j's DTO is frozen.

## Non-goals (unchanged from design.md)

- No removal of `/analysis` / `/filter` / `/steering` rule-data CRUD (that's rule
  data, not settings).
- No live `AnalyzerRegistry` rebuild on flip (analysis stays OnRestart).
