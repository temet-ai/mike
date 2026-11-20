---
title: Central System-Configuration Page — Full ProxyConfig Surface
status: draft
created: 2026-06-16
updated: 2026-06-16
summary: Make the /settings page the single place to set every operator-tunable ProxyConfig field, generalizing the existing 6-toggle mechanism (DB override → overlay/startup-fold → session freeze).
session_id:
  - 23214930-40a5-41bd-b773-3d13e30b10b8
---

# Central System-Configuration Page

## Why

Operator decision (2026-06-16): the `/settings` page exposes only the 6 binary feature toggles, but the
config that actually controls system functionality is "sprinkled like fairy dust across the system"
(`/analysis`, `/filter`, `/steerers`, boot-time-only fields). The operator chose **Everything
(A+B+C)**: `/settings` becomes the one central page to set the full operator-tunable `ProxyConfig`
surface. Bound by the no-default-off rule (every feature toggle defaults ON and is UI-settable) and the
reactivity rule (session-scoped settings freeze per session and never re-evaluate per turn; workers/caps
apply on restart).

## Architecture — generalize, don't fork

The 6-toggle mechanism is the template:
`config.feature_toggles` (DB) → `FeatureToggleService` (DB-first upsert, write-guard mutex) →
`Arc<ArcSwap<FeatureToggles>>` overlay → session freeze via `SessionToggleCache` /
`ResolvedToggles::from_overlay`. REST is metadata-driven (`TOGGLE_META` → DTO carrying scope + applies);
the frontend renders generically from that metadata.

Generalized:

1. **Storage.** Keep `config.feature_toggles` (6-key closed CHECK) exactly as-is — it is load-bearing and
   its session-freeze path works. Add a parallel `config.settings` table (`key TEXT PK, value JSONB NOT
   NULL, updated_at TIMESTAMPTZ`) for the *expanded* surface. No closed CHECK (the key set grows); keys
   are validated Rust-side against `SETTINGS_META`. Unify the two stores only at the REST catalogue and
   the frontend.

2. **Metadata registry.** `SETTINGS_META: &[SettingMeta]` — one entry per tunable field:
   `{ key (dotted path), group, kind (Bool|Int|Float|String|StringList|Enum), scope, applies, default,
   label, description, min/max/step? }`. Drives validation AND the generic frontend.

3. **Reactivity follows where the value is read** (this is the whole correctness argument):
   - **Boot/spawn readers → apply-on-restart.** The subsystem already reads `state.config.<field>` once at
     spawn. To make a UI flip take effect, fold the `config.settings` DB overrides onto the `ProxyConfig`
     struct at `AppState::new` *before* subsystems are built. No overlay, no per-turn cost, no rewiring.
     A PATCH writes the DB row; the next restart applies it.
   - **Per-turn hot-path readers → session-frozen.** Extend `ResolvedToggles` (frozen once per session in
     `SessionToggleCache`) to carry the session-scoped params. First-session-sight snapshots the live
     overlay; per-turn reads the frozen snapshot. A flip affects only sessions that start afterward.
     These fields DO need the live `ArcSwap` overlay so new sessions see the flip without a restart.

## Reactivity classification (derived from read-site, not invented)

| Group | Setting(s) | kind | scope | applies | read site |
|---|---|---|---|---|---|
| Session features (existing) | `reminders.enabled`, `recall_injector.enabled`, `weasel_intercept.enabled` | Bool | Session | NewSessions | hot path (frozen) |
| Workers (existing) | `steerer.enabled`, `lesson_worker.enabled`, `harvester.enabled` | Bool | Global | OnRestart | spawn |
| Reminders params | `reminders.inject_lessons` (Bool), `reminders.max_lessons_to_inject` (Int) | mixed | Session | NewSessions | hot path (frozen) |
| Weasel params | `weasel_intercept.forbidden_words` (StringList), `weasel_intercept.correction_prompt` (String), `weasel_intercept.max_buffer_bytes` (Int) | mixed | Session | NewSessions | hot path (frozen) |
| Recall injector param | `recall_injector.max_lessons_per_topic` (Int) | Int | Session | NewSessions | hot path (frozen) |
| Composition | `composition_enabled` (Bool) | Bool | Global | OnRestart | spawn |
| Embedding | `embedding.enabled` (Bool), `embedding.url` (String), `embedding.model` (String), `embedding.chunk_max_chars` (Int) | mixed | Global | OnRestart | settings page / restart |
| Analysis | `analysis.enabled` (Bool); per category (overselling, justifying_shortcuts, deflecting_blame, avoiding_work, faking_understanding): `enabled` (Bool) + `false_positive_filters` (StringList) | mixed | Global | OnRestart | AnalyzerRegistry baked at startup |
| Steerer params | `steerer.model` (String/Enum), `steerer.request_timeout_secs` (Int), `steerer.quiet_period_secs` (Int), `steerer.refocus.enabled` (Bool), `steerer.refocus.confidence_threshold` (Float), `steerer.refocus.debounce_turns` (Int) | mixed | Global | OnRestart | spawn |
| Lesson worker params | `lesson_worker.dedup_threshold` (Float), `max_lessons_per_project` (Int), `session_consolidation_idle_secs` (Int), `nightly_consolidation_hour` (Int), `gc_interval_secs` (Int) | mixed | Global | OnRestart | spawn |
| Cap manager | `cap_manager.rss_ceiling_bytes` (Int), `rss_high_watermark_pct` (Int), `rss_low_watermark_pct` (Int), `cap_manager.interval` (humantime) | mixed | Global | OnRestart | spawn |
| Resource caps | `max_request_body_bytes`, `max_response_body_bytes`, `max_in_flight`, `max_store_bytes` (Int), `spa_origins` (StringList) | mixed | Boot | OnRestart | startup |
| Logging | `log_level` (Enum), `log_format` (Enum), `log_dir` (String?), `log_max_files` (Int) | mixed | Global | OnRestart | startup |

**EXCLUDED from the UI surface:**
- `steerer.openrouter_api_key` — **secret** (`SecretString`, `skip_serializing`). Never surfaced; supplied via env. Show a read-only "configured via env: yes/no" indicator at most.
- `steerer.force_fire` — **TEST/DEBUG** firehose (no-default-off carve-out, stays false). Not a feature.
- `sweep_at_startup` — destructive interlock + recorded operator approval (registry exception). Stays OFF; not surfaced as a normal toggle.
- `cap_manager.rss_ceiling_bytes` default `0` and similar tuning thresholds are NOT feature gates — `0` = observation-only is a legitimate tuning value, not an off-by-default feature.

## File allowlist (by stage)

**Stage 1 — db-dev**
- `migrations/<ts>_config_settings.sql` (new) — `config.settings` table.
- `.sqlx/` regenerated entries (via `cargo sqlx prepare --workspace`).

**Stage 2 — rust-developer**
- `crates/claude-proxy/src/settings/mod.rs` (new) — `SettingMeta`, `SettingKind`, `SettingValue`, `SETTINGS_META`, validation.
- `crates/claude-proxy/src/settings/service.rs` (new) — `SettingsService` (mirror of FeatureToggleService).
- `crates/claude-proxy/src/store/persistence/settings.rs` (new) — `fetch_settings` / `upsert_setting`.
- `crates/claude-proxy/src/store/persistence/mod.rs` — register the new persistence module.
- `crates/claude-proxy/src/config/mod.rs` — fold DB overrides onto `ProxyConfig` at `AppState::new`; extend `ResolvedToggles` + `SettingsWriteGuard`; AppState fields for the settings overlay + service.
- `crates/claude-proxy/src/api/handlers.rs` — `GET /api/settings` (catalogue) + `PATCH /api/settings/{key}`; keep `/api/settings/features[/{key}]` working.
- `crates/claude-proxy/src/api/mod.rs` — route registration.
- `crates/claude-proxy/src/api/manifest.rs` — manifest entries (Exempt from MCP per 18.4 default).
- `crates/claude-proxy/src/lib.rs` — module declaration if needed.
- Reading wiring at the per-turn freeze sites (reminders/weasel/recall_injector) and the startup fold.

**Stage 3 — frontend-developer**
- `frontend/src/lib/api/settings.ts` — catalogue DTO + `getSettings()` / `patchSetting(key, value)`.
- `frontend/src/routes/settings/+page.svelte` — generic grouped, metadata-driven render.
- `frontend/src/lib/components/settings/*` (new) — control components per kind (toggle/number/text/string-list/enum) + group section.

**Stage 4 — ui-designer**
- `frontend/src/routes/settings.stories.svelte` — respec for the full grouped surface.

**Stage 5 — docs**
- `/settings` — header note: `/settings` is the full configuration surface.
- `docs/ARCHITECTURE.md` — settings subsystem paragraph.

## Staging / dependency order

1. db-dev migration (additive, no dependency).
2. rust-developer backend (depends on 1 for `.sqlx`).
3. frontend (depends on 2's REST shape).
4. ui-designer story (parallel-able with 3; finalize after 3).
5. docs (last).

## Non-goals (this thread)

- Removing the config controls already on `/analysis`, `/filter`, `/steerers`. The filter-rule and
  steerer-rule CRUD on those pages is *rule data*, not settings, and stays. Only the genuinely
  *config-valued* knobs move/centralize. A later pass can replace overlapping config controls with links
  to `/settings`.
- Live AnalyzerRegistry rebuild on flip (analysis stays OnRestart).
