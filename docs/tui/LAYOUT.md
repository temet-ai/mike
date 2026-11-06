---
title: TUI Design — Layout
status: active
created: 2026-07-23
updated: 2026-10-21
summary: The portable TUI Design layout — one universal app bar, panel frame, navigation-only help bar, overflow, and resize behavior.
---

# TUI Design — Layout

This document describes **layout only** for the portable, application-neutral **TUI
Design** system. Application screen specs are layered on top; mike appears only as a
labelled example. Color, typography, keybinding sets, and panel-internal content conventions
are separate decisions and do not belong here. (Operator correction 2026-07-24.)

[`LAYOUT.html`](LAYOUT.html) draws the layout at four sizes with a labelled mike example
(80×24, 160×40, 60×20 and 50×14). This is that page as drawn in a browser; regenerate the
HTML with [`build_layout_doc.py`](build_layout_doc.py), then the picture with
`scripts/html-doc-render`:

![The universal layout at 80×24, 160×40, 60×20 and 50×14: app bar, content-sized nav, middle and detail panels splitting the rest, help bar; at 60×20 the detail panel stacks below, at 50×14 it opens as a modal](layout/page.png)

## The grid

- A screen is a grid of character cells. Every measurement in this document is in cells:
  columns × rows.
- The design floor is **80×24**. Every screen is designed at the floor first; larger sizes
  get more content space, never a different layout.

## Screen anatomy

Every screen has the same three regions, in the same order, always:

```
┌────────────────────────────── 80 columns ──────────────────────────────┐
│ app bar    Mike / settings / tokens                              1 row │
├──────────────┬───────────────────────────────────┬────────────────────┤
│ nav          │ middle                            │ detail/preview     │
│              │                                   │                    │
│              │                                   │                    │
├──────────────┴───────────────────────────────────┴────────────────────┤
│ help bar — navigation commands                                   1 row │
└────────────────────────────────────────────────────────────────────────┘
```

### App bar

- One row, full width — it goes from one side of the screen to the other.
- The app bar always shows **where the user is in the navigation**, readable at a glance,
  as a path from the app root. Application example (mike): `Mike / settings / tokens`.
- It never scrolls and never grows.

### Content region

- Everything between the app bar and the help bar.
- Divided into three panels, left to right: **nav**, **middle**, **detail/preview**.

### Help bar

- One row, full width, last row of the screen.
- Contains the correct information to help the user **navigate**: navigation commands for
  the current screen.
- **No statuses allowed.** The help bar contains navigation help only. System state,
  counts, connection, and progress go elsewhere.

## Panels

- A panel is a rectangle of whole cells with a box-drawn border one cell thick.
- **Each panel has its name in the top of the panel, in the border.**
- **The panel's name is the name of the item it is showing**, not the panel's role.
  Navigated to settings: the middle panel is titled `settings`. With `tokens` selected
  there: the detail/preview panel is titled `tokens`.
- **Each panel name carries a highlighted shortcut that comes from the name**: one
  character of the name is the panel's jump shortcut and is rendered highlighted within
  the name. The shortcut character must be a letter of the name itself.
- Panels can contain further panels, depending on what is being shown. Nested panels
  follow every rule in this section: bordered, named in the border, shortcut in the name.
- **nav is never wide.** It is sized to its content and borders — a respectable size that
  gets out of the way. Space nav does not need belongs to middle and detail/preview.
- Panel positions are fixed per screen. Panels never move, swap, or collapse in response
  to data. An empty panel keeps its position and size.
- **Navigation via Tab always goes from left to right** (then wraps). Nested panels are
  entered in the same left-to-right order.

## Selection

- **If a row is active, it is highlighted.** The highlight is the row treatment itself,
  across the row — selection adds no geometry: no marker column, no leading symbol, no
  indent shift.

## Panel internals — next phase

Panel-internal content conventions — including table and row structure, cell padding,
gaps, banding, spacing, and alignment — are deliberately undefined. They are the next
TUI Design phase and will be developed in the future generic `tui-kit` repository, then
made enforceable. Do not infer those rules from the illustrative mike HTML rendition.
(Operator correction 2026-07-24.)

## Overflow

- Content taller than its panel scrolls inside the panel. The panel does not grow and the
  layout does not change.
- Content is never silently cut. Scrolling and pagination over truncation —
  `.claude/rules/agent-behavior-and-data.md`
  binds every content surface.

## Resize

- At the floor (80×24) everything in the screen spec fits exactly.
- Resize is purge-and-re-render at the new size, not reflow.
- The app bar and help bar exist at every size: always one row, always full width.

### Wider and taller than the floor

- **nav keeps its content-sized width** — navigation lists do not benefit from width.
- The extra columns split **equally between middle and detail/preview** (a screen spec may
  state different proportions).
- Extra rows go to the content region; panels get taller, positions unchanged.
- Nothing recenters and nothing caps: on a widescreen monitor the layout is the same three
  panels, wider.

### Narrow real estate

When the area becomes narrow, the layout reflows based on the real estate on screen —
never by squeezing all panels thinner:

- **Right items move below left items.** The rightmost panel stacks below the panel to its
  left (detail/preview moves below middle), each keeping its full usable width.
- **If stacking is not possible** — not enough rows for the stacked panels — **the panel's
  view becomes a modal**: it opens over the screen when the user navigates to it, and
  closes back to the panels beneath it.
- The app bar and help bar always remain: one row each, full width.

## Screen specs against this document

An application screen spec states this document's variables for one screen: the navigation
path shown in the app bar, the three panels' widths and names (with their shortcut letters),
any nested panels, and the wider-than-floor proportions. It must not present
application-specific table, row, padding, gap, banding, spacing, or alignment choices as
TUI Design rules while the Panels phase is reserved.
