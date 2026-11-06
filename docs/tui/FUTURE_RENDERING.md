---
title: TUI Design — Future rendering options
status: draft
created: 2026-10-02
updated: 2026-10-08
summary: The direction for mike's fleet interface — a mike mod inside Claude Code, with SVG graphs where the surface draws them — plus what the terminal can draw of FUNK, the techniques tested in foot, and the alternatives that were weighed.
---

# TUI Design — Future rendering options

**Direction (2026-10-02): the fleet interface becomes a mike mod inside Claude Code.**
Projects, fleets, a fleet's status, plan and setup, and the sessions at work are drawn in a
pane docked beside the operator's own conversation, in every Claude Code app. The graphs
are SVG where the app draws SVG, and rounded box-drawing panels (`╭ ╮ ╰ ╯`) in the terminal.
The rest of this document is the research behind that choice and the alternatives weighed.

**Built so far (2026-10-08):** step 2, the fleet's status, lives in the `fleet-comms` mod
(`mods/fleet-comms/`), not in a mod of its own: `/fleet-comms` opens one fleet pane with
the status at the top, drawn as `mike fleet status` draws it (`hooks/panel.tsx`, a port of
`crates/mike/src/launch/fleet/render/`), and the messages below. The command stays
`/fleet-comms` because `/fleet` is the command that writes a fleet. NEEDS YOU and NOTHING
IS MOVING reach the status line and the band above the prompt. Steps 3 to 6 are not built.

## What it lets us do

- See every project's fleets and sessions without leaving the session you are working in:
  `/fleet` opens a pane beside the transcript, and the status line says when a fleet needs you.
- Read a fleet's setup and its live status, stop or start it, and answer what it asks you,
  from the same pane.
- See the plan as a graph and the seats at work as a graph with real FUNK edges, corners and
  fonts in the desktop app, VS Code and the mobile app; a box-drawn version in the terminal.
- Open a seat's transcript and send it a message from the pane; jump to its herdr pane to
  type into it live.
- Give the same interface to anyone running Claude Code with mike, with nothing extra to
  launch: the mod ships with mike.

## The mike mod

Claude Code (2.1.287) loads **mods**: plugins whose hooks module, written in TSX, draws
inside Claude Code. Session 72beac08 built the first mike mod on 2026-10-02 (a `/seats`
pane over the workspace roster, unread mail on the status line, `/recall`), which is how
this was found. The API is in the types the `plugin-authoring` skill writes
(`claude-code.d.ts`); what follows is what it offers the fleet interface.

### What a mod can draw

| Element | Where | Use for the fleet |
|---|---|---|
| `Pane` | docked beside the transcript (terminal in fullscreen from 110 columns), else inline above the prompt; tabs when several are open; follows the main or an agent's transcript | the fleet interface itself |
| `Box`, `Text`, `Button`, `Link`, `Code`, `Markdown` | every surface | panels, rows, keys, briefs |
| `Input`, `Select` | all but mobile | the message composer, choosing a fleet or project |
| `Svg` | desktop, VS Code, mobile | the plan DAG and the seats-at-work graph in real FUNK |
| `Raster` | terminal | a repaintable cell grid, e.g. context bars or a box-drawn graph |
| `Image` | terminal, where the terminal shows pixels (kitty, Ghostty); `alt` text elsewhere | not relied on: foot under herdr shows none (see below) |
| `Client` | terminal, desktop | a region a module of the mod's own draws, with pointer events |
| `AbovePrompt`, `$.ui.status`, `$.ui.toast` | every surface | NEEDS YOU and NOTHING IS MOVING without opening the pane |

Boxes take rounded borders, colours and hover groups. A hook on a built-in tool's render
(`SendMessage`, `ListAgents`, …) can restyle how fleet mail shows in the transcript.

### What a mod can reach

`$.http.fetch` to mike's inspect API on :9989, `$.clock.every` for refresh, `$.command`
for slash commands, `$.process` for commands, `$.state` / `$.store` for values a drawing
reads. Everything the TUI mockups show already has a route: `GET /api/fleets`,
`/api/fleets/{fleet}`, `/api/fleets/{fleet}/status` (`FleetStatus`, with NEEDS YOU and
NOTHING IS MOVING), `/start`, `/stop`, `/changes`, the workspace roster, a session's
requests and messages.

### What it cannot do

- **Attach to another session's live terminal.** No element runs a second terminal. The
  pane shows a seat's transcript from mike's capture and sends it mail; typing into the
  seat live stays a herdr job (`herdr agent focus` / `attach`), started from a Button.
- **Pixels in foot under herdr.** `Image` needs kitty graphics through herdr and a terminal
  that shows them; foot does not.

### Plan

1. **Ship the mod with mike.** One mod in the repository (e.g. `mods/mike/`), installed by
   `make install` alongside `mike-mcp`; fold in session 72beac08's `/seats`, inbox line and
   `/recall`.
2. **Fleet list and status.** `/fleet` opens the pane: projects → fleets → a fleet's STATUS,
   read from `FleetStatus`, refreshed on a timer; NEEDS YOU on the status line and above the
   prompt.
3. **Setup.** The SETUP tab from the fleet definition (`GET /api/fleets/{fleet}`).
4. **Plan and seats as graphs.** `Svg` on desktop, VS Code and mobile, laid out from the
   definition's `after` edges and the fleet's mail; a box-drawn `Raster` or `Box` version in
   the terminal.
5. **Sessions.** A seat's transcript and a message composer (`Input`) in the pane; a Button
   that focuses the seat's herdr pane.
6. **Stop asks first.** Stop in the pane names the seats that are mid-answer and waits for
   a yes.

mike's ratatui TUI (bare `mike`) keeps the stack screens (Services, Config, Setup,
Sessions). It does not grow a fleet screen; the mockups for one
(`claude.ai/artifact/MVAgiYT9Kio7rkvHzd3Dju`) carry over to the pane's layout.

## How Claude Code draws its terminal UI

Claude Code (2.1.284, one Bun executable) runs React 19.2 against a heavily modified fork
of Ink, with Yoga flexbox layout ported to TypeScript. Each frame goes React tree → layout →
a 2-D cell grid held in packed typed arrays → diff against the previous frame → the
smallest ANSI write, wrapped in synchronized output (DEC mode 2026). It draws in the normal
screen by default so the terminal's scrollback and selection keep working;
`CLAUDE_CODE_NO_FLICKER=1` switches to the alternate screen.

React gives Claude Code a way to organise a large TypeScript component tree. It gives
nothing visual a terminal lacks: Ink draws the same box characters ratatui does. ratatui
already renders into a cell buffer and writes only changed cells. Worth taking for mike:
wrap each draw in synchronized output (crossterm `BeginSynchronizedUpdate` /
`EndSynchronizedUpdate`), and redraw when data changes rather than on a fixed tick.

## What the terminal stack can draw

Sessions run in **foot 1.28 → herdr 0.9.1 → the session**, font JetBrainsMono Nerd Font.

- foot draws these ranges itself, filling the cell with no gaps: U+2500–259F (box drawing,
  blocks), U+2800–28FF (braille), U+1CD00–1CDE5 (octants), U+1FB00–1FB9B (Symbols for
  Legacy Computing: sextants, wedges, eighth blocks).
- The font supplies the Nerd Font tab ends (U+E0B4–E0BF). It lacks ◢◤ (U+25E2–25E5) and the
  diagonal box lines U+1FBA0–1FBAF, which come from a fallback font.
- foot shows sixel images; herdr passes only kitty graphics (herdrdev/herdr#4330). No image
  reaches the screen from a herdr pane in foot. Ghostty → herdr would show kitty graphics,
  with herdr's open image bugs (#3676, #3877, #4677).

## Techniques, tested in foot

Seen in foot at 16 pt with JetBrainsMono Nerd Font; the test sheet and screenshot are in the
artifacts repository, `artifacts/2026-10-02/terminal-borders-and-graphics/`.

| Technique | Result |
|---|---|
| Rounded box drawing `╭╮╰╯` | Clean. **The current choice.** |
| `╱` as a corner | Does not join the frame lines. Rejected. |
| Outline chamfered with 🮣 🮢 🮡 🮠 (U+1FBA0–A3) | Joins the frame at every corner, from a fallback font. Four cut corners read closest to FUNK's edge. |
| Filled panel cut with complementary wedges, 🭊 + 🭠 (wide) or 🭋 + 🭡 (steep) | Drawn by foot, no seam. The top-left cell fills its lower right and the bottom-right cell its upper left, along the same diagonal. |
| Font triangles ◢ ◤ | Smaller than the cell, visible gaps. Rejected. |
| Frame on a filled background | Fill shows square behind the cut. Rejected; keep border cells on the page colour. |
| Chips: slanted or round Nerd Font ends, or plain | All clean. |
| Graph edges in box lines with rounded turns and ▶ | Read well. |
| Braille and octant lines | Too faint at normal size for edges. |
| Half-block and eighth-block frames (ratatui `QUADRANT_OUTSIDE`, `ONE_EIGHTH_WIDE`) | Hug the cell edge; usable around a filled panel. |

Further options in the terminal, not tested:

- **A FUNK glyph font** in the Private Use Area (cut-corner pieces at FUNK's angle, chip ends,
  the brand mark), as herdr does with `Herdr Agent Icons Max`. Every machine installs it.
- **Kitty text sizing** (OSC 66, kitty 0.40+) for larger headings; kitty only as far as found.
- **Pixel images** of the graph through `ratatui-image` and the kitty protocol: ghostty or
  kitty only, not foot.

## Libraries

Versions as on crates.io, 2026-10-02.

### In the terminal

| Library | Use |
|---|---|
| ratatui 0.30 | what the TUI uses; every technique above |
| tui-term 0.3.4, vt100 0.16, portable-pty 0.9 | a live session in a TUI panel, by running `herdr agent attach` in a pty |
| tui-big-text 0.8 | block-letter headings |
| OpenTUI | TypeScript (React or Solid) on a Zig core; opencode's renderer. Still a cell grid |
| Ink | Claude Code's renderer. Still a cell grid |

### A GPU-drawn window in Rust

A window that shows live sessions needs a terminal engine and a UI library; Zed's terminal
panel is GPUI drawing `alacritty_terminal`.

| Library | Notes |
|---|---|
| alacritty_terminal 0.26 | the terminal engine Zed uses |
| libghostty-vt 0.2.2 | Ghostty's engine; unstable, needs zig |
| egui 0.36 | simplest; egui_term 0.1 for terminals; egui_ratatui 2.2 runs the existing ratatui screens unchanged |
| iced 0.14 | iced_term 0.8 for terminals, kept up more than egui_term |
| Slint 1.18 | its own markup for layout and styling; GPU or software rendering |
| GPUI (gpui-box 0.1.1) | Zed's framework; unofficial standalone copy, pre-1.0 |
| Makepad 1.0 | shader-driven GPU UI |
| vello 0.10 | GPU 2-D drawing for real edges and cut corners |
| ratatui-wgpu 0.6 | ratatui on the GPU; still a cell grid |

### A webview window

Tauri 2 (3.0 in alpha) puts the SPA in a desktop window. FUNK is already built there as
Svelte components, the project session graph already uses Svelte Flow, and live updates
come over the inspect WebSocket. A live session would be xterm.js connected through mike to
`herdr agent attach` over a WebSocket. Ratzilla 0.3 runs ratatui screens in a browser.

## Alternatives weighed

Before mods were found, the recommendation was a Tauri window around the SPA, where FUNK
exists already, with iced and iced_term as the Rust alternative. The mod wins because it
puts the interface where the operator already works, draws real SVG on three of four
surfaces without a second app, and ships with mike. The Tauri window remains the route if
the interface ever needs a live attached terminal in the same view, which a mod cannot run.
In the terminal, rounded panels are the choice; cut corners stay available later through the
chamfered-outline or wedge-cut technique above.

## Sources

- HN, Claude Code TUI engineer: https://news.ycombinator.com/item?id=46701013
- The Signature Flicker: https://steipete.me/posts/2025/signature-flicker
- CLAUDE_CODE_NO_FLICKER: https://slyapustin.com/blog/claude-code-no-flicker.html
- Ken Huang, Terminal UI Architecture: https://kenhuangus.substack.com/p/chapter-6-terminal-ui-architecture
- Unicode, Symbols for Legacy Computing: https://www.unicode.org/charts/PDF/Unicode-17.0/U170-1FB00.pdf
- Cascadia Code sextants and octants: https://devblogs.microsoft.com/commandline/cascadia-code-2404-23/
- kitty text sizing protocol: https://sw.kovidgoyal.net/kitty/text-sizing-protocol/
- herdr documentation index: https://herdr.dev/llms.txt
- libghostty: https://mitchellh.com/writing/libghostty-is-coming
- GPUI: https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md
- ratatui-wgpu: https://docs.rs/ratatui-wgpu
- egui_ratatui: https://github.com/gold-silver-copper/egui_ratatui
- OpenTUI: https://opentui.com/
- Claude Code mods: the `plugin-authoring` skill and its `claude-code.d.ts` (Claude Code 2.1.287); the first mike mod, `~/.claude/dev-mods/72beac08-3986-48bd-8563-522811350732/mike/`
