---
title: mike TUI — Start / Stop screen spec
status: active
created: 2026-07-20
updated: 2026-07-20
summary: Layout, data sources, states, and keymap for the Start / Stop lifecycle screen — the TUI face of the #63 ensure-up/serve engine.
---

# Start / Stop

Visual source of truth: `start-stop.html` (design project
`903f9ba4-4fb6-46c1-a1e7-90bba166a25f`, viewport 1200×760, subtitle "mike running, stop
selected"). The mock lives in that design project, not in this repository; this spec
is the readable version of it. Style rules: see [`../STYLE_GUIDE.md`](../STYLE_GUIDE.md). Requirements
floor: §3 of `artifacts/2026-07-20/tui-page-requirements.md`.

## Purpose

Lifecycle without scripts — the TUI face of the #63 engine. One shared implementation;
the TUI adds no second lifecycle logic.

## Entry

Navigation from Home's `menu` (Start or Stop item) or the direct `s`/`x` hotkey.

## Screen accent

`rust` (levers/actions) — `Block title="lifecycle" tone="rust"`,
`List accent="var(--rust)"` on the Stop/Start actions. Note: `StatusBar mode="RUNNING"`
still uses `modeColor="var(--acid)"` — that badge reports live-state, not the screen's
own lever accent; see style guide §2.

## Layout

Two-column half split (`1.15fr 1fr`), same grid as Home, collapsing to one column below
860px.

**Left column:**

| Panel | Hotkey | Contents | Live data source |
|---|---|---|---|
| `ports` | `p` | same binds `Table` as Home: [bind, address, state] × 3, `state` `● up` | the same three-bind health probe Home uses |
| `log` (grow) | `l` | scrollable plain log lines (timestamp + message), 10 sample rows: proxy/inspect/spa ready, db pool ready, mcp attached (core profile, 50 tools), 4× capture-stored, 1× recall-surfaced | requirements floor: "last few log lines or the log path" |

Footer on `ports`: `KeyHint [⏎ open url]`.

**Right column:**

| Panel | Hotkey | Contents | Live data source |
|---|---|---|---|
| `lifecycle` (`tone="rust"`, grow) | `s` (fallback bracket form — "lifecycle" contains no `s`, see style guide §5) | state line `● running · since 14:32 · pid 48120 · uptime 2h14m`; interactive `List` of 2 actions: Stop, Start (dim, "already running") | current pid/uptime from the #63 engine |

Footer on `lifecycle`: note text "quitting the tui never stops the stack — the system
just runs."

**Design note — two overlapping input paths for the same actions.** The `StatusBar`
exposes direct hotkeys `x` (stop) and `r` (restart) that aren't mirrored in the
`lifecycle` panel's `List` (which only offers Stop/Start, no Restart item). This may be
intentional — a direct key for power users alongside list navigation for
discoverability — but it isn't spelled out in the requirements floor. Confirm the
intended behavior (does `r` restart exist as a real action, and should the list gain a
Restart item?) before implementing both paths as-is.

## Behavior

Start is validated and fail-safe: DB pre-check with plain remediation before boot; a
failed start leaves prior healthy state alone. Stop never touches processes mike didn't
spawn. Quitting the TUI never stops the stack (stated directly in the `lifecycle` panel
footer).

## StatusBar

`mode="RUNNING"` (acid) · message: "stack up · pid 48120 · 2h14m" · keys: `[x stop]
[r restart] [l logs] [? help]` · `onShortcuts` wired.

## ShortcutsModal sections

- **global:** `j/k` move selection · `⏎` select · `⇥` cycle panels · `⌃ /` this modal
- **start / stop:** `x` stop the stack · `r` restart · `l` view logs · `q` quit

## States

Only the **running, Stop actionable / Start dimmed** state is designed. Not designed,
per the requirements floor:

- **Stopped** — Start actionable, Stop dimmed or absent, no pid/uptime line.
- **Starting** (in-progress) — directional motion only (no pulsing, per style guide §6).
- **Already-running attempt to Start** — "Start becomes no-op with 'already healthy'."
- **Start failure** — "remediation text inline + jump-to-fix (Setup DB step or
  Config)." No mock; likely a `Block alert` (rust accent + subtle border runner, style
  guide §5) inside or below the `lifecycle` panel.
