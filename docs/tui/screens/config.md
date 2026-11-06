---
title: mike TUI — Config screen spec
status: active
created: 2026-07-20
updated: 2026-07-20
summary: Layout, data sources, states, and keymap for the Config screen — view and edit the live runtime settings store, same store the SPA's /settings edits over REST.
---

# Config

Visual source of truth: `config.html` (design project
`903f9ba4-4fb6-46c1-a1e7-90bba166a25f`, viewport 1200×760, subtitle "live runtime
settings, postgres store"). The mock lives in that design project, not in this
repository; this spec is the readable version of it. Style rules: see [`../STYLE_GUIDE.md`](../STYLE_GUIDE.md).
Requirements floor: §4 of `artifacts/2026-07-20/tui-page-requirements.md`.

## Purpose

View and change the live runtime configuration — the same store the SPA's `/settings`
edits over REST. No parallel config system, no file editing.

## Entry

Navigation from Home's `menu` → Config.

## Screen accent

`ice` (configure) — `Block title="settings" tone="ice"`, `Table accent="var(--ice)"`,
`StatusBar mode="CONFIG" modeColor="var(--ice)"`.

## Layout

Single full-bleed `Block title="settings"` (`hotkey="s"`, `tone="ice"`, grow, `scroll`),
containing repeated group sections — an uppercase group label (`--font-label`, ice) then
a `Table` (columns: setting / value / apply, `widths="32ch 12ch 1fr"`) per group:

| Group | Rows |
|---|---|
| capture | `capture.enabled` (on), `capture.store_bodies` (on) |
| steering | `steering.enabled` (on), `steering.openrouter_key` (unset — rendered `—` in `--fg-dim`) |
| compression | `compression.enabled` (on) |
| memory | `memory.recall_enabled` (on), `memory.embedding_server` (`ollama`) |
| mailbox | `mailbox.enabled` (on) |

Boolean values render `on` in acid / `off` in rust. The `apply` column shows
`immediate` (plain) or `applies on restart` (muted) per row — the same honest
apply-semantics the SPA shows.

**Footer lines inside the Block** (not separate UI states — static copy):
"anything not editable here → open the web ui at http://127.0.0.1:9990/settings" and
"if the live store is unreachable, this view falls back to read-only boot config." The
second line *describes* the fallback state in prose; the fallback itself has no visual
design (see States below).

**Design note — capability rows not shown.** The requirements floor's content floor for
Config includes "capability rows for context," but the finished screen renders none —
only the settings table. Either the finished design deliberately leaves capability
context to Home only, or a capability panel is still owed here; confirm before
implementing.

## Data

`GET /api/settings` (catalogue: groups, kinds, values, apply-semantics) +
`GET /api/settings/features` (toggles) + `PATCH` for writes.

## StatusBar

`mode="CONFIG"` (ice) · message: "live settings · postgres store" · keys: `[j/k move]
[⏎ edit] [/ filter] [? help]` · `onShortcuts` wired.

## ShortcutsModal sections

- **global:** `j/k` move selection · `⇥` cycle panels · `⌃ /` this modal
- **config:** `⏎` edit selected setting · `/` filter settings · `esc` back to control
  center

## States

Only the **populated, live-store, no row mid-edit** state is designed. Not designed,
per the requirements floor:

- **API down** — "read-only view of boot config with a 'live store unreachable' banner +
  Start shortcut." The Block footer's second line describes this in prose but no banner
  or read-only visual treatment exists yet.
- **Row being edited** — bools toggle, scalars get a validated input; no mock shows an
  actively-editing row.
- **Write failure** — "inline error on the row, value reverts." No mock.

Mind style guide §8 when implementing real settings data: the `32ch` setting-key column
and `12ch` value column will silently ellipsis-clip a longer key or value than the
sample data shown here — widen the columns or move long values out of the default
`Table` cell rendering.

## Gaps vs the requirements floor (2026-07-21 review)
- **Explicit quit** (`tui-page-requirements.md:17`, shared frame on every page): Config has no `q`
  quit key; the other three screens do. `esc` here only returns to the control center, not quit.
  Add `q` to Config's StatusBar/ShortcutsModal to match, or disambiguate esc-vs-quit.
- **`esc` back-to-Home**: Config binds `esc` → control center; no other screen documents a
  return-to-Home key, and it isn't surfaced as a StatusBar chip. Promote it to a STYLE_GUIDE §7
  cross-screen convention applied consistently, or scope to Config with a stated rationale.
- **Focus glow on ice**: no `--glow-ice` token exists (STYLE_GUIDE §2), yet Config defaults to ice
  tone — a focused ice Block renders border + corner-ticks WITHOUT glow unless `--glow-ice` is
  defined. Decide at implementation (same applies to Setup).
- **Capability rows** (floor §4): the floor lists "capability rows for context" on Config; the
  design shows settings only (capabilities live on Home). Confirm the split with the operator.
