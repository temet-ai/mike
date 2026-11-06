---
title: mike TUI — Home / Status screen spec
status: active
created: 2026-07-20
updated: 2026-07-23
summary: Layout, data sources, states, and keymap for the Home / Status screen — the control center bare `mike` opens on a configured machine.
---

# Home / Status

Style rules: see [`../STYLE_GUIDE.md`](../STYLE_GUIDE.md), especially the status
vocabulary rule at `.claude/rules/visual-design-and-ui.md`. Requirements floor:
§1 of `artifacts/2026-07-20/tui-page-requirements.md`. The original mockup
(`mike-control-center.html`, kept in the design project, not in this repository) was a starting point, not the current source of truth — this
spec describes the implementation as it stands, which has diverged through several rounds
of live design feedback (menu-disappears-on-nav, mismatched status vocabulary, misaligned
rows — see git history on `crates/mike/src/tui/screens/home.rs` and `app.rs`).

## Purpose

Answer "is my system up and honest?" in one glance; route to every other page.

## Entry

Bare `mike` on a configured machine. An unconfigured machine lands in
[Setup](setup.md) instead.

## Screen accent

`acid` (liveness) — `StatusBar mode="ONLINE" modeColor="var(--acid)"`, top-bar header
`● online · configured`. The SERVICES panel is `tone="uv"`; the persistent nav rail is
`tone="rust"`.

## Layout

Three pieces, top to bottom, all owned by `App::draw` (`crates/mike/src/tui/app.rs`) except
the SERVICES panel body:

1. **Top bar** (`draw_top_bar`, Home-owned, one row): app name left, `● online/stopped ·
   configured` right.
2. **Nav rail** (`App`-owned, persistent across Home/Config/Setup — never Home's own
   concern, never disappears on navigation): a fixed 18-column `List` with exactly three
   items, `Services` / `Config` / `Setup`. Arrow keys/`j`/`k`/`g`/`G` move the highlight
   **and** switch the active screen live — no `Enter` required. `Tab` hands keyboard focus
   to the active screen's content; `Shift+Tab` returns it to the rail. Hidden only on the
   `StartStop` drill-down (reached via `l` from Services), which keeps the full frame.
3. **SERVICES panel** (content-sized, not stretched — blank space below on a tall
   terminal), one grouped `Block` with three plainly labeled sections, all sharing one
   status vocabulary (`.claude/rules/visual-design-and-ui.md`):
   - **BINDS** — one row per bind (`proxy · /v1` `127.0.0.1:9988`, `inspect · /api`
     `127.0.0.1:9989`, `web ui` `127.0.0.1:9990`), glyph-leading: `● up` / `○ down`.
   - **SYSTEM** — `store` (byte usage/entry count), `process` (RSS peak), `mike-mcp`
     (on-PATH + inspect-reachable), as aligned `label   value` rows.
   - **CAPABILITIES** — one row per `GET /api/capabilities` entry, same glyph-leading
     shape as BINDS: `● up` (was `Active`), `○ degraded`, `○ inert`, `○ off` — no fix hint
     rendered here (see "Capability fix hints" below).

Bottom `StatusBar` is a fourth, separate row (unchanged position, still app-level chrome
per `render_status_bar`).

## StatusBar

`mode="ONLINE"` (acid) · message: `"stack up · 3 binds healthy"` (`· steering inert`
appended when the steering capability is inert) · keys are contextual to the stack's own
state: `[t] stop` (or `[t] start` when stopped) · `[r] restart` (only shown while
running) · `[l] logs` · `[s] setup` · `[q] quit` · `onShortcuts` wired (⌃/⌘+/,
`[⌃ /] shortcuts` chip). These are the *only* place the toggle/restart/logs keys are
hinted — the SERVICES panel's own status line states fact only (`● stack running · pid
64362 · uptime 3h`), never a duplicate key hint.

## ShortcutsModal sections

- **nav rail:** `↑↓/jk` browse Services/Config/Setup · `⇥` focus this panel · `⇧⇥` back
  to the rail
- **services:** `t` toggle services · `r` restart services · `l` open the full service
  screen (logs) · `s` setup · `q` quit

## Capability fix hints

Decision (2026-07-23): a capability's remediation text (e.g. "add forbidden words to
Weasel intercept") does **not** render on Home. Home is a glance screen; the specific
fix belongs on the Config screen that owns the setting. This retires the "jump-to-fix
deep-link" gap noted in an earlier review of this doc — there is no fix hint on Home left
to deep-link from.

## States

**Only the fully-healthy/populated state is designed** in the finished mockup. The
per-page requirements floor calls for two more states with no visual yet — implement
them against this same layout/grammar, not as a new design:

- **Stack down entirely** — still renders; per the requirements floor it "shows
  down-state + offers Start." Binds render with `○ down` (red, per style guide §2's
  `--red` — the one error/red token defined there); the SERVICES panel's own status line
  and the bottom StatusBar both switch their toggle key to `[t] start`.
- **API unreachable but process alive** — "error banner with the probe result, retry
  key." No mock exists; the style guide's `Block alert` (§5) — rust accent + subtle
  border runner — is the documented mechanism for this kind of banner, unused so far.

## Gaps vs the requirements floor
- **OSC-8 clickable bind URLs** (floor `:36-37`): the BINDS rows show plain address
  strings; OSC-8 hyperlinking is an implementation-time concern not yet done (same for
  `start-stop.md`'s ports table).
