---
title: TUI Design
status: active
created: 2026-07-23
updated: 2026-10-21
summary: Normative rules for portable TUI Design — one universal layout, panels (reserved), SilkCircuit colors, animation (reserved with no rules), navigation and help, and best practices for good UI.
---

# TUI Design

**Status:** Normative. TUI Design is portable and application-neutral. Application screen
specs are layered on top; in this repository, mike's control center, setup flow, and any
future terminal inspector are application examples. Whoever implements a TUI screen is
bound by the rules below. A screen spec or
implementation that violates a rule without a written operator exception is Wrong, not a
style variation. (Operator correction 2026-07-24.)

**Source:** Codified from Stefanie Jane, *"The Terminal Renaissance: Designing Beautiful TUIs
in the Age of AI"* (hyperbliss.tech, 2026-04-04) and the SilkCircuit design language
(github.com/hyperb1iss/silkcircuit). **Operator decision 2026-07-23: the TUI adopts
SilkCircuit as its design language.** The terminal does NOT derive its palette from the SPA
web tokens — the SPA and the TUI are intentionally different visual systems, and this
document (not `docs/frontend/`) is the source of truth for every terminal surface.

Rules are numbered (`TUI-L*` layout, `TUI-PN*` panels, `TUI-C*` colors, `TUI-G*` glyphs,
`TUI-K*`/`TUI-P*` navigation/principles, `TUI-A*` architecture, `TUI-B*` best practices,
`TUI-V*` verification) so specs, reviews, and commit messages can cite them. IDs are stable:
a rule keeps its number when sections reorganize.

**Designed rendition:** [`docs/tui/STYLE_GUIDE.html`](STYLE_GUIDE.html) — the same rules
rendered in SilkCircuit itself (palette swatches, the universal layout shown with a
labelled mike example at 80×24, tier
demos, Wrong/Right pairs). Rule IDs match; this markdown stays the normative
text, the HTML is the visual reference. GitHub shows the HTML as source, so each of its
sections is also a picture, shown in [The designed rendition](#the-designed-rendition) at
the end of this page. Regenerate the HTML via
[`docs/tui/build_style_guide.py`](build_style_guide.py) — its width assertions verify every
mockup line at build time, so box-drawing alignment is proven, not eyeballed — then the
pictures with `scripts/html-doc-render`.

---

## 1. Layout (TUI-L)

**TUI-L1 — One universal layout, defined in
[`docs/tui/LAYOUT.md`](LAYOUT.md) (operator decision 2026-07-23).** Every application uses
the same layout; there is no per-screen layout choice. The design: an app bar edge to edge
always showing the navigation path; a content region of three panels left to right —
content-sized nav, then middle and detail/preview splitting the rest equally — titled by
the item each is showing, each title carrying a highlighted shortcut letter from its name;
and a navigation-only help bar in the final row. The help bar never carries application
status, counts, connection state, or progress. Narrow real estate reflows by stacking right
items below left items, and when stacking is impossible the panel's view becomes a modal.
`LAYOUT.md` is the normative text; its HTML rendition shows the layout at four sizes with
a labelled mike application example.

**TUI-L2 — Design for 80 columns.** Every cell matters in a way pixels don't; a single wasted
column is a percentage of the screen. Minimum supported size is 80×24: every screen spec
carries an exact 80×24 mockup plus one wider reference size, and states its behavior at narrow
widths. Information density is a feature, not a problem — every cell on screen earns its place.

**TUI-L3 — Multi-mode apps keep one frame.** When one app has multiple modes, all modes
share the universal layout and panel positions; only the content is mode-specific. Six
modes must not feel like "six apps wearing a trench coat."

**TUI-P1 — Spatial consistency is the foundation.** Panels don't move. Tabs stay in order.
The user builds a mental map in the first minute and navigates by location memory after that.
The layout never rearranges without explicit user action — every shuffle resets their spatial
model to zero. Spatial memory *is* the navigation.

## 2. Panels (TUI-PN)

What lives inside a panel. **This is the next design phase; nothing below is defined yet.**
Panel-internal content conventions — including table and row structure, cell padding,
gaps, banding, and spacing — must be worked out in the future generic `tui-kit` repository
and made enforceable there. Until then, do not infer those rules from the illustrative mike
mockups or add them elsewhere in this guide. (Operator correction 2026-07-24.)

### Tables

### Data

### Graphics

### Toggles

### Views

### Logs

### Modals

### Inputs

### Form options

## 3. Colors (TUI-C)

**TUI-C1 — The golden rule: usable at 16 colors, beautiful at true color.** Color in a
terminal is a resource, not a paintbrush.

**TUI-C2 — Three capability tiers, explicit fallbacks.**
- *16 ANSI colors* — the foundation and the SSH-over-a-bad-connection baseline. Named ANSI
  colors inherit the user's terminal theme: automatic coherence, no fine control.
- *256 colors* — fixed extended palette; control without theme coherence. Use sparingly for
  emphasis; never build the whole palette here.
- *True color (24-bit)* — where it gets beautiful, but always an **enhancement layer over the
  16-color foundation, never a replacement for it**.

Every color in the design system states its truecolor value *and* its 256-color fallback
(the screen-spec contract: "never spec a capability without its degradation").

**TUI-C3 — Detection and courtesy contract.** Check `$COLORTERM` for `truecolor`/`24bit`;
check `$TERM` for `256color`; **respect `$NO_COLOR` unconditionally** — if set, strip all
color. That is professional courtesy, not just accessibility.

**TUI-C4 — Colors are defined by function, not appearance.** No widget code references a raw
color; everything goes through a semantic slot ("focused panel borders use `accent.primary`,"
never "`this border is #rrggbb`"). Adopted slot vocabulary:

| Slot | Meaning |
|---|---|
| `text.primary` | Main body text |
| `text.muted` | Secondary info, metadata, timestamps — noticeably dimmer |
| `text.emphasis` | Headers, focused items — bright, bold |
| `bg.base` → `bg.surface` → `bg.overlay` | Three background layers, each ~5–8% lighter; depth without borders |
| `accent.primary` | Brand; interactive elements, focused borders |
| `accent.secondary` | Supporting interactions, secondary highlights |
| `status.success` / `.warning` / `.error` / `.info` | Exactly what they sound like |
| Domain slots (e.g. `diff.added` / `diff.removed`) | Added per domain as needed, same pattern |

**TUI-C4a — Slot values: the published SilkCircuit Neon tokens.** Every value below is a
published token from the SilkCircuit **Neon** variant table
(`lua/silkcircuit/variants.lua` in github.com/hyperb1iss/silkcircuit) — the source column
names the exact token. Nothing is invented: the README's flat `#0a0a0f`/`#e0e0e0` teaser
palette is NOT the theme — the real Neon backgrounds are **purple-tinted**
(`#0a0812 → #12101a → #1a162a`), primary text is warm white `#f8f8f2`, secondary text is
lavender, and muted text is blue-violet, not grey. Using neutral greys here reproduces a
generic dark theme and is Wrong. 256-color fallbacks are the nearest xterm-256 index
(TUI-C2).

| Slot | Value | 256 fallback | Neon token — usage |
|---|---|---|---|
| `bg.base` | `#0a0812` | 232 | `bg_dark` — the app canvas |
| `bg.surface` | `#12101a` | 233 | `bg` — panels, cards |
| `bg.overlay` | `#1a162a` | 235 | `bg_highlight` — modals, overlays |
| `bg.selection` | `#44475a` | 239 | `selection` / `bg_visual` — selected rows, visual ranges |
| `text.primary` | `#f8f8f2` | 255 | `fg` — main body text |
| `text.secondary` | `#e9d5ff` | 189 | `fg_dark` — lavender; ledes, secondary emphasis |
| `text.muted` | `#6272a4` | 61 | `fg_gutter` — metadata, labels, timestamps |
| `text.emphasis` | `#ffffff` bold | 231 | `fg_light` — headers, focused items |
| `accent.primary` | `#e135ff` | 171 | `purple` — brand, focus states, keywords |
| `accent.muted` | `#9580ff` | 105 | `purple_dark` — non-focused accent structure |
| `accent.secondary` | `#80ffea` | 122 | `cyan` — interaction, paths, links (`cyan_bright #5fffff` / 87) |
| `accent.tertiary` | `#ff6ac1` | 205 | `coral` — accents, hashes (`#ff79c6` strings, `#ff00ff` pink for glow moments / 201) |
| `status.success` | `#50fa7b` | 84 | `green` — confirmations, additions, online |
| `status.warning` | `#f1fa8c` | 228 | `yellow` — warnings, attention (diagnostic `warning #ecc48d`) |
| `status.error` | `#ff6363` | 203 | `red_error` — danger, offline (diagnostic `error #ff5874`) |
| `status.info` | `#82aaff` | 111 | `info` / `blue` — informational |
| `data.number` | `#ffb86c` | 215 | Warm Orange (README) — numbers, counts, constants |
| `diff.added` / `diff.removed` | `#50fa7b` / `#ff6363` | 84 / 203 | `diff_add` / `red` — additions / deletions |
| `glow.purple` / `glow.pink` / `glow.cyan` | `#e135ff` / `#ff00ff` / `#80ffea` | 171 / 201 / 122 | `glow_*` — glow emphasis |

**TUI-C4b — Intensity variants, not ad-hoc tweaks.** SilkCircuit ships five variants of the
same semantic slots, each a full published palette in `variants.lua`: **Neon** (the
TUI-C4a default), **Vibrant** (`bg #0f0c1a`, purple `#ff00ff`, cyan `#00ffcc`),
**Soft** (`bg #1a1626`, purple `#e892ff`, cyan `#99ffee`), **Glow** (`bg #0a0816` toward
black, pure `#ff00ff`/`#00ffff`/`#00ff00`, glow rendering), and **Dawn** (light: `bg
#faf8ff`, fg `#2b2540`, purple `#7e2bd5`). Wanting the app dimmer/brighter means selecting a
*variant* through the theme pipeline (TUI-C5) — never hand-adjusting slot values on one
screen. Same semantic slots, different energy: "the design language is the mapping from
meaning to color, not the colors themselves."

**TUI-C5 — Theming is infrastructure, not hex codes in source files.** The resolution pipeline
is *palette (raw colors) → tokens (semantic names) → styles (composed fg + bg + modifiers) →
gradients (multi-stop interpolation)*, each layer referencing only the one below. This is what
makes the app theme-swappable and keeps multiple TUIs visually consistent from one definition.
Themes are validated by contract tests (every token defined, every style resolving) — a theme
that half-resolves is a build failure, not a runtime surprise. "Hex codes in source files is a
phase, not a strategy."

**TUI-C6 — State machines get a visual language.** When a screen classifies items into states
(hot / waiting / ready / stale / blocked, request outcomes, session health), each state maps
to a color *and* a label *and* a style — consistently, everywhere that state appears. The
screen should look different when things are on fire versus calm. Color as information, not
decoration.

**TUI-P4 — Semantic color.** Color carries meaning, not decoration: green = success, red =
danger/error, amber = caution. Color only *reinforces* hierarchy already established by
layout, typography, and symbols — **if stripping all color makes the app unusable, the design
is broken.** (This is also the monochrome tier of TUI-P7 and the `NO_COLOR` contract of
TUI-C3.)

**TUI-P7 — Design in layers.** Build and evaluate in this order, each tier independently
usable: (1) monochrome — is it usable with no color at all? (2) 16 ANSI colors — is the
hierarchy readable? (3) true color — is it beautiful? The app must work on a monochrome SSH
session and look good in a modern emulator. That is a design discipline, not a tradeoff.


### Glyphs (TUI-G)

**TUI-G1 — Documented glyph vocabulary; no indicator glyphs.** A TUI carries state in
**text with semantic color** (`live`, `busy`, `idle`, `529 overloaded`) — never in round
indicator dots (`●`/`○`/`◐`/`◉`) and never in selection markers (`▸`): selection is the
row treatment (LAYOUT.md), and TUI-G5's geometry extends to round glyph shapes. The
documented vocabulary is small and each glyph has exactly one meaning: `·` inline
separator, `—` null (never blank), fractional blocks `▏▎▍▌▋▊▉█` for bars, block elements
`▁▂▃▄▅▆▇█` for sparklines, box-drawing for panel borders. Additions extend this table
deliberately. Glyphs stay within the unambiguous-width set: double-width characters and
ZWJ sequences (which includes most emoji) break terminal cell alignment — an engineering
constraint, not a style ban.

**TUI-G5 — No pills, no rounded chrome, no desktop window dressing.** The terminal is a
square cell grid; the design language's geometry is rectangular everywhere it appears —
including web/doc renditions of the system (this guide's HTML rendition, dashboards, story
mockups of TUI screens). `border-radius`, pill-shaped badges/chips, macOS traffic-light
window dots, and faux window chrome are banned outright. Labels that need containment use
square borders or bracketed tokens (`[ status: active ]`), matching the box-drawing idiom.
(Operator correction 2026-07-23.)

**TUI-G4 — Full-fidelity content.** The project's inspection-fidelity rule (`scripts/fidelity-check`,
`docs/decisions/inspection-fidelity-catalogue.md`) binds every content surface: scrolling and pagination over truncation, never `…`-elision of
inspection content. (Repo rule; stricter than the source article and it wins.)

## 4. Animation

**Reserved.** No rules are defined.

## 5. Navigation and help (TUI-K)

**TUI-P2 — Keyboard-first, mouse-optional.** Every feature is reachable without a mouse, and
mouse support is not an afterthought either. Power users are keyboard people; beginners
discovering the app will click. Support both; optimize for keys.

**TUI-P3 — Progressive disclosure, three tiers.** (a) Help bar with the 3–5 most important
keys, always visible. (b) `?` help overlay with the full keybinding reference, on demand.
(c) Complete documentation for everything else. Beginners see the floor; experts find the
ceiling; nobody reads a manual to get started. A "wall of keyboard shortcuts" on screen is
Wrong.

**TUI-P6 — Contextual intelligence.** The interface reflects what the user is doing *now*:
keybindings change as focus moves between panels, the help bar shows the commands actually
available in this context, and the `?` overlay matches it. The UI earns trust by
always being accurate about what's possible.


**TUI-K1 — Vim keys are the lingua franca.** `j`/`k` up/down, `h`/`l` left/right, `/` search,
`?` help, `g`/`G` top/bottom, `Esc` back. Six keystrokes handle 80% of navigation; they are
muscle memory for exactly the audience that uses TUIs. Every list, table, and scrollable
region supports them.

**TUI-K2 — Four interaction layers, each invisible until reached for** (progressive
disclosure applied to the keyboard):

| Layer | Keys | Discoverability |
|---|---|---|
| L0 Universal | Arrow keys, `Enter`, `Esc`, `q` quit | Help bar |
| L1 Vim navigation | `j`/`k`/`h`/`l`, `/`, `?`, `:` | Help bar |
| L2 Actions | Single mnemonic keys (`d` delete, `r` refresh, …) | `?` help overlay |
| L3 Power | Composed commands, macros, configuration | Documentation only |

**TUI-K3 — Keymap rules.** L2 action keys are mnemonic single letters, consistent across
screens (the same action never has two keys on two screens). Mode/screen switching uses a
consistent scheme (e.g. `Shift+letter`) so switching builds muscle memory. Every screen spec
lists its keymap by layer.

## 6. Best practices for good UI

**TUI-P5 — Async everything.** Never freeze the UI. File I/O, network calls, DB queries:
background tasks with progress indicators. The user can always press `Esc` and get a
responsive interface back. A TUI that hangs is a TUI that gets killed.

**TUI-A1 — Never poll; push.** The UI is told when something changes (watch/broadcast
channels, reactive streams); it does not ask "has anything changed?" on a timer. The
responsiveness difference is visceral.

**TUI-A2 — Unidirectional data flow: events → state → render.** State transitions are pure
(`(state, event) → (new state, side effects)`), no I/O inside the reducer. External inputs
(proxy events, agent output) flow through the same event system as keystrokes. This is what
makes the UI testable, debuggable, and auditable — and it is the pattern both major TUI
ecosystems converge on.

**TUI-A3 — Know when not to be a TUI.** Follow the dual-product pattern: one core, two faces —
a scriptable CLI (plain/JSON output, composable with pipes, zero decoration) and an
interactive TUI for humans. When stdout is a pipe, render nothing decorative: no color, no
box-drawing. The discipline is in *not* rendering things when the context doesn't
want rendering. (Application example: mike already uses `mike-mcp` as
its CLI face.)

**TUI-B1 — AI-friendly UI.** AI agents build, drive, and verify this TUI, and they are
blind by default — so the UI must be operable and checkable without human eyes: every state
change flows through the event system (TUI-A2) so an agent can reason about and replay it;
every feature is reachable by keys (TUI-P2) so an agent can drive it; the CLI face (TUI-A3)
exposes the same data scriptably; and the rendered screen is verifiable through a real
terminal (TUI-V1). "The loop closes: design → build → run → see → fix → repeat."

**TUI-V1 — Close the loop with eyes.** An agent building a TUI is blind by default — stdout
text cannot verify that borders line up, that `j`/`k` navigates, or that the final help row
renders correctly. Every TUI change is verified by *running the app in a real
terminal and reading the rendered screen* (pty harness / snapshot tests / terminal-automation
tooling — pty and snapshot testing; ghostty-automator is the
reference model for emulator-level state inspection). The loop is design → build → run → see
→ fix → repeat. A passing unit test is not a substitute for seeing the screen — consistent
with this repo's verification-before-completion rule.

**TUI-V2 — Verify per layer.** The TUI-P7 tiers are each testable claims: snapshot the screen
monochrome, at 16 colors, and at truecolor; verify at 80×24 and at the spec's wider reference
size. A screen that only ever ran truecolor-fullscreen is unverified.

---

## Screen-spec checklist

Every screen spec under `docs/tui/screens/` (and every review of one) checks:

- [ ] Uses the universal layout — app bar path, nav/middle/detail-preview with contextual
      titles, navigation-only help bar in the final row (TUI-L1, `docs/tui/LAYOUT.md`)
- [ ] Exact 80×24 mockup + wider reference, narrow-width behavior stated (TUI-L2)
- [ ] Keymap listed by layer L0–L3; help bar shows 3–5 keys; `?` overlay contents defined (TUI-K2, TUI-P3)
- [ ] All colors named as semantic slots with 256-color fallback; no raw values (TUI-C2, TUI-C4)
- [ ] Usable-when-stripped: hierarchy holds in monochrome (TUI-P4, TUI-P7)
- [ ] Every glyph used is in the documented vocabulary (TUI-G1)
- [ ] Loading/empty/error/populated states specified; nothing blocks the event loop (TUI-P5)
- [ ] Focus-dependent keybindings enumerated; the help bar matches the current navigation
      context and carries no status (TUI-P6)
- [ ] No truncation of inspection content — scroll/paginate (TUI-G4)

## Wrong / Right (drift-prone patterns)

| Wrong | Right | Rule |
|---|---|---|
| Rearranging panels when data changes ("collapse the empty one") | Fixed positions; empty panel shows its empty state | TUI-P1 |
| Help bar listing every keybinding | 3–5 keys in the help bar, rest behind `?` | TUI-P3 |
| Red text as the only signal a request failed | State word + `status.error` color | TUI-P4 |
| Blocking the render loop on a DB query | Background task + progress indicator, `Esc` responsive | TUI-P5 |
| `Style::default().fg(Color::Rgb(…))` at a call site | Style resolved from a semantic slot | TUI-C4 |
| Shipping only a truecolor look | 16-color fallback declared per token, `NO_COLOR` honored | TUI-C2, TUI-C3 |
| Polling the inspect API on a tick to redraw | Push via watch/broadcast channel | TUI-A1 |
| "Tests pass" as TUI completion claim | Rendered-screen evidence from a real terminal | TUI-V1 |
| Pill badges, rounded cards, window dots on any rendition of the system | Square corners, box borders, bracketed tokens | TUI-G5 |

## References

- **Full reference index:** [`docs/tui/REFERENCES.md`](REFERENCES.md) — every link from the
  source article, canonical homes for everything it names, the portable TUI Design stack,
  and the mike application stack.

## The designed rendition

Each section of [`STYLE_GUIDE.html`](STYLE_GUIDE.html), as drawn in a browser. The rules
above are normative; these pictures are the visual reference.

![The design language at a glance](style-guide/01-the-design-language-at-a-glance.png)
![Layout — one design, TUI-L1](style-guide/02-layout-one-design-tui-l1.png)
![Panels, TUI-PN](style-guide/03-panels-tui-pn.png)
![Palette, TUI-C4a](style-guide/04-palette-tui-c4a.png)
![Intensity variants, TUI-C4b](style-guide/05-intensity-variants-tui-c4b.png)
![Color tiers and detection, TUI-C1 to C3](style-guide/06-color-tiers-detection-tui-c1-c3.png)
![Theming is infrastructure, TUI-C5 and C6](style-guide/07-theming-is-infrastructure-tui-c5-tui-c6.png)
![Glyph vocabulary, TUI-G1 to G5](style-guide/08-glyph-vocabulary-tui-g1-g5.png)
![Animation](style-guide/09-animation.png)
![Navigation and help, TUI-K and TUI-P2, P3, P6](style-guide/10-navigation-and-help-tui-k-tui-p2-p3-p6.png)
![Best practices for good UI](style-guide/11-best-practices-for-good-ui-tui-p5-tui-a-tui-b-tu.png)
![Screen-spec checklist](style-guide/12-screen-spec-checklist.png)
![Wrong / Right — the drift-prone patterns](style-guide/13-wrong-right-the-drift-prone-patterns.png)
