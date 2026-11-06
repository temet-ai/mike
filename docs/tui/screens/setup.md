---
title: mike TUI — Setup screen spec
status: active
created: 2026-07-20
updated: 2026-09-16
summary: Layout, data sources, states, and keymap for all seven steps of the guided Setup flow — the terminal step chooses and live-validates the backend `mike spawn` launches into.
---

# Setup (guided flow)

Visual source of truth: design project `903f9ba4-4fb6-46c1-a1e7-90bba166a25f`, viewport
1200×760, six screens (one per step; the terminal step has no mock and follows the
database step's list shape). The mock files named below live in that design project, not
in this repository; this spec is the readable version of them. Style rules: see
[`../STYLE_GUIDE.md`](../STYLE_GUIDE.md). Requirements floor: §2 of
`artifacts/2026-07-20/tui-page-requirements.md`.

| Step | Mock in the design project | Subtitle |
|---|---|---|
| 1 · Welcome | `setup-welcome.html` | first-run guided setup, step 1 of 7 — welcome |
| 2 · Dependency probe | `setup-deps.html` | first-run guided setup, step 2 of 7 — dependency probe |
| 3 · Terminal | — (no mock) | first-run guided setup, step 3 of 7 — choose and validate the terminal backend |
| 4 · Database | `setup.html` | first-run guided setup, step 4 of 7 — choose a database path |
| 5 · MCP registration | `setup-mcp.html` | first-run guided setup, step 5 of 7 — register mike-mcp tools |
| 6 · Optional unlocks | `setup-unlocks.html` | first-run guided setup, step 6 of 7 — openrouter key + embedding server |
| 7 · Exit / ready | `setup-exit.html` | first-run guided setup, step 7 of 7 — success, options-first ways to run mike |

## Purpose

From nothing to a green system without reading docs. First run (no config found) lands
here automatically; re-enterable any time from the Home menu.

## Entry

No config found on launch, or explicit navigation from Home's `menu` → Setup.

## Shared chrome

Every step shares one header: brand (`mike`, uv, `fx-cursor`) + crumb `"setup · first
run"` + a 7-step indicator, one `Step` per wizard step — completed steps render `◆`
acid, the current step renders `◈` with its label (`"N · name"`), upcoming steps render
`○` in `--fg-dim`. Every step wires `?` / `⌃ /` to toggle its `ShortcutsModal`
(`onShortcuts`).

## Screen accent

`ice` (configure) for steps 1–6 — `Block tone="ice"`, `StatusBar mode="SETUP"
modeColor="var(--ice)"`. **Step 7 shifts to `acid`** (liveness/success) — `Block
tone="acid"`, `StatusBar mode="READY" modeColor="var(--acid)"`, and its indicator glyph
carries `text-shadow: var(--glow-acid)`.

---

## Step 1 — Welcome

**Screen.** `setup-welcome.html`.

**Purpose.** What mike is, what setup will do — no marketing (floor
`tui-page-requirements.md:55`).

**7-step indicator.** `◈` step 1 (`"1 · welcome"`, ice); steps 2–7 `○` dim, unlabeled.

**Layout.** Single `Block title="welcome" tone="ice" grow appear="slide-in"` — no
`hotkey`, not `focusable` (nothing to select). Static content, no live data:
- Intro line: "mike captures and inspects the claude api traffic on your machine, and
  can steer it."
- "this setup will:" (ice label) followed by 6 bullet rows (`▸` glyph, ice): check your
  dependencies · choose the terminal backend sessions launch into · choose where captures
  are stored · register the mike-mcp tools in claude code · unlock optional features
  (steering, semantic recall) · show you the ways to run mike. These six map 1:1 to steps
  2–7.
- Footer (border-top, ice): "press ⏎ to begin."

**Keymap.** StatusBar: `[⏎ begin] [? help] [q quit]` (no `[←]` — first step, nothing to
go back to). ShortcutsModal — global: `j/k` move selection · `⏎` select · `⇥` cycle
panels · `⌃ /` this modal; setup: `⏎` begin · `q` quit setup.

**States.** One state only — static welcome text. No data fetch, nothing to fail.

---

## Step 2 — Dependency probe

**Screen.** `setup-deps.html`.

**Purpose.** Existing Postgres reachable? Docker present? brew present? `claude` on
PATH? Each row found/missing + what it enables; missing items never block the flow
(floor `:56-58`).

**7-step indicator.** Step 1 `◆` acid (done); `◈` step 2 (`"2 · dependencies"`, ice);
steps 3–7 `○` dim.

**Layout.** Intro line: "mike scans this machine for optional dependencies. nothing here
blocks setup — missing items just narrow later choices." Then `Block title="dependencies"
hotkey="d" tone="ice" focusable grow` → a read-only `Table` (`[dependency, status, what
it enables]`, widths `28ch 11ch 1fr`), data source = probes (ports, `command -v` — floor
`:82`):

| Row | Status | Enables |
|---|---|---|
| homebrew | `●` found (acid) | native postgres + tool installs |
| docker | `●` found (acid) | the docker database path |
| postgres (127.0.0.1:5432) | `○` not found (dim) | managed-url / native-brew path will provision one |
| claude (on PATH) | `●` found (acid) | required for `mike claude` |
| herdr | `●` found (acid) | the herdr terminal backend |
| tmux | `●` found (acid) | the tmux backend |
| xdg-terminal-exec | `●` found (acid) | the plain-terminal backend |
| hyprctl | `●` found (acid) | the plain-terminal backend |

Footer: "missing items only disable options later — they never block setup."

**Keymap.** StatusBar: `[⏎ continue] [← back] [? help]` (no `[j/k]` — the table is a
read-only display, not a selectable `List`, unlike steps 3 and 4). ShortcutsModal — global:
`j/k` move selection · `⏎` select · `⇥` cycle panels · `⌃ /` this modal; setup: `←` back
a step · `⏎` continue · `q` quit setup.

**States.** Only the fully-resolved state is designed (3 found, 1 missing); the four
terminal rows render with the same row treatment and no mock. See Gaps.

---

## Step 3 — Terminal

**Screen.** No mock; the layout follows step 4's list.

**Purpose.** Choose the terminal backend `mike spawn` launches Claude Code sessions into,
as a genuine choice with one-line trade-offs, and prove it live before the wizard moves
on. Setup never continues with a backend that failed validation; `none` is always
available and says what it costs.

**7-step indicator.** Steps 1–2 `◆` acid; `◈` step 3 (`"3 · terminal"`, ice); steps 4–7
`○` dim.

**Layout.** Intro line: "choose the terminal backend mike launches claude code sessions
into. mike validates it live before continuing." `Block title="terminal" hotkey="t"
tone="ice" focusable grow` → interactive `List` of one `Path` item per `BackendId::ALL`
entry, in that fixed order, `accent="var(--ice)"`, `j/k` moves selection. Each row is
name + status on one line and the trade-off underneath; the status comes from the
backend's `probe()`, run for every backend with an adapter (`backend_for` is `Some`)
when the step is entered, all at once, each bounded by its own subprocess timeouts:

| Backend | Trade-off | Status |
|---|---|---|
| herdr | "persistent server with agent states and a session-uuid join; the fleet's first backend" | `●` ready / `○` not found — `<first failed check's remedy>` |
| tmux | "headless hosts, ssh, containers; no display needed, no agent state" | `●` ready / `○` not found — `<first failed check's remedy>` |
| terminal | "plain windows via xdg-terminal-exec + hyprctl; window placement only, no agent state" | `●` ready / `○` not found — `<first failed check's remedy>` |
| ghostty | "ghostty windows on macos via applescript (later build)" | `○` not in this build |
| none | "hand-launch only: `mike claude` works, `mike spawn` refuses, the fleet protocol is unavailable" | `●` ready |

Before a probe answers the status cell reads `probing` with no glyph. Rows whose status
is `not in this build` render in `--fg-dim` and cannot be chosen.

**Conditional panel** (shown once a validation has run on this visit to the step): a
`Block title="validation · <backend>"` under the list with one row per check in the
report's order, never trimmed: `● <name> — <detail>` for a passed check, `○ <name> —
<detail> · fix: <remedy>` for a failed one. Leaving and re-entering the step re-probes
and clears the panel.

Footer (one line, `--fg-3`): "validation opens a scratch workspace in the backend and
closes it again."

**Data / engines beyond this screen.** `probe()` and `validate()` on
`launch::TerminalBackend` (the herdr adapter opens a labelled scratch workspace, runs a
marker command, reads it back, lists agents and closes the workspace). A passed
validation lands in `.env` as `MIKE_LAUNCH_BACKEND=<id>` through the same
`persist_env_var` path as the other steps, mirrored into the process env. The
validation report is not written to the database from here — the database is chosen in
step 4, after this — so the first `mike spawn doctor` records it in
`launch.backend_validations`.

**Keymap.** StatusBar: `[j/k choose] [⏎ validate] [← back] [? help]`. ShortcutsModal —
global: `j/k` move selection · `⏎` select · `⇥` cycle panels · `⌃ /` this modal; setup:
`←` back a step · `⏎` validate backend · `q` quit setup. `⏎` on:
- a `not in this build` row: no-op; banner "`<id>` is not in this build yet; choose herdr
  or none".
- `none`: persists `MIKE_LAUNCH_BACKEND=none` and advances to step 4, no validation.
- a `ready` row: `busy` for the duration with the banner "validating"; then the checks
  panel. Passed → persist and advance to step 4. Failed → stay, panel visible, banner
  "validation failed — fix it and press ⏎ to retry, or choose another backend".
- a `not found` row: probes that backend again instead of advancing (a server started
  meanwhile becomes `ready`); banner "`<id>` is ready — press ⏎ again to validate it" or
  "`<id>` is still not found — `<remedy>`".

**States.** probing (no probe has answered) · probed (ready / not found / not in this
build per row) · validation failed (checks panel visible, step unchanged) · validation
passed (step advanced, `.env` written). Each has a render test in
`crates/mike/src/tui/screens/setup/terminal_tests.rs`; none has a mock.

**Gaps.** Checks land as one report after `validate()` returns rather than one by one as
they complete, and the "validating" banner is set for the awaited call but the event loop
draws only after it returns — the same in-place await the other steps use. No
per-check progress and no mock.

---

## Step 4 — Database

**Screen.** `setup.html`.

**Purpose.** The four DB paths as a genuine choice with honest trade-offs, none hidden
as "advanced" (floor `:59-69`).

**7-step indicator.** Steps 1–3 `◆` acid; `◈` step 4 (`"4 · database"`, ice); steps 5–7
`○` dim.

**Layout.** Intro line: "choose how mike stores captured traffic. all four are genuine
options — pick what fits." `Block title="database" hotkey="d" tone="ice" focusable grow`
→ interactive `List` of 4 `Path` items (name + description), `accent="var(--ice)"`,
`j/k` moves selection, data source = the dependency probes from step 2 (Docker/brew
green gates two of the four paths):

| Path | Description | Requires |
|---|---|---|
| trial · ephemeral | "no db — dies with the process. the 30-second first taste." | nothing (`MIKE_PERSIST_ENABLED=0`) |
| docker container | "pgvector image via scripts/local-db/up.sh. needs docker running." | Docker probe green |
| native brew | "PostgreSQL 19 + pgvector. Use Docker if the native packages are unavailable." | brew probe green |
| managed url | "paste a postgres url — neon, supabase, rds. you manage it." | a reachable connection string |

Descriptions wrap via the `Path` component's own `white-space: normal` (overriding
`List`'s default `nowrap`+ellipsis — see style guide §8) rather than clipping.

**Conditional panel** (shown only when "managed url" is selected — `sel===3`): a
`db-next` block with label "managed url · postgres connection string", an `Input`
(`accent="var(--ice)"`, `autoFocus`, placeholder `postgres://user:pass@host:5432/db`),
and a note "neon, supabase, rds all carry pgvector."

**Data / engines beyond this screen.** Per the requirements floor: the #63
ensure-up/serve engine, the migration runner, settings REST (`PATCH
/api/settings/...`), the `~/.claude.json` writer (floor `:82-84`). **None of these is
visually represented here** — this screen only shows the pre-execution choice; running
the chosen path (container pull / brew install / connection test) and the subsequent
migration-with-progress step belong to this step's continuation, undesigned (see Gaps).

**Keymap.** StatusBar: `[j/k choose] [⏎ continue] [← back] [? help]`. ShortcutsModal —
global: `j/k` move selection · `⏎` select · `⇥` cycle panels · `⌃ /` this modal; setup:
`←` back a step · `⏎` continue · `q` quit setup.

**States.** Only the pre-execution choice state is designed (no path chosen yet,
"managed url" shown selected as the interesting/expanded case). See Gaps.

---

## Step 5 — MCP registration

**Screen.** `setup-mcp.html`.

**Purpose.** Register `mike-mcp` into `~/.claude.json` with explicit consent shown
before writing; skippable; shows exactly what will be written (floor `:70-71`).

**7-step indicator.** Steps 1–4 `◆` acid; `◈` step 5 (`"5 · mcp registration"`, ice);
steps 6–7 `○` dim.

**Layout.** Single `Block title="mcp registration" hotkey="m" tone="ice" focusable grow`,
static consent display, data source = the `~/.claude.json` writer (floor `:83`):
- Explain line: "register the mike-mcp tools into claude code so agents can query
  captured traffic (sessions, requests, bodies, tool calls)."
- Consent panel, label "will be added to ~/.claude.json:", pre-formatted code block:
  ```
  "mcpServers": {
    "mike": { "command": "mike-mcp", "args": [] }
  }
  ```
- Note: "skippable — you can register later from config. nothing else in
  ~/.claude.json is touched."

**Keymap.** StatusBar: `[⏎ register] [s skip] [← back] [? help]`. ShortcutsModal —
global: `j/k` move selection · `⏎` select · `⇥` cycle panels · `⌃ /` this modal; setup:
`←` back a step · `⏎` register · `s` skip this step · `q` quit setup.

**States.** Only the pre-write consent state is designed. See Gaps.

---

## Step 6 — Optional unlocks

**Screen.** `setup-unlocks.html`.

**Purpose.** OpenRouter key (steering & judges) and embedding server (semantic recall):
each states plainly what it unlocks and what stays inert without it — same language as
the capability states; both skippable, key entry masked, values land in the live
settings store (floor `:72-75`).

**7-step indicator.** Steps 1–5 `◆` acid; `◈` step 6 (`"6 · optional unlocks"`, ice);
step 7 `○` dim.

**Layout.** Intro line: "both integrations are optional — mike runs fully without them.
add a key now or skip and set it later." `Block title="optional unlocks" hotkey="u"
tone="ice" focusable grow`, two `unlock` rows, data source = settings REST (`PATCH
/api/settings/...` — floor `:83`):

| Unlock | Desc | Input | Inert-without note |
|---|---|---|---|
| openrouter key (`◇` rust) | "unlocks steering (mid-flight nudges) and the llm judges." | masked `Input`, `accent="var(--ice)"`, `autoFocus`, placeholder `sk-or-…` | "without it, steering stays `○ INERT`." |
| embedding server (`◇` rust) | "unlocks semantic recall (similar-turn lookup)." | `Input` pre-filled `http://localhost:7997`, placeholder "http://localhost:7997 or ollama" | "without it, recall is lexical-only." |

Footer: "both are optional — skip and add them later in config. keys are stored in the
live settings, never printed."

**Keymap.** StatusBar: `[⏎ save] [s skip] [← back] [? help]`. ShortcutsModal — global:
`⏎` save · `⇥` cycle panels · `⌃ /` this modal (no `j/k` here — two `Input`s, not a
selectable list); setup: `←` back a step · `s` skip step · `q` quit setup.

**States.** Only the initial/unfilled state is designed (openrouter key masked-empty,
embedding URL pre-filled with the default). See Gaps.

---

## Step 7 — Exit / ready

**Screen.** `setup-exit.html`.

**Purpose.** Capability panel result (what's Active vs deliberately Inert), then the
options-first "ways to use mike" list, side by side — teaches the `mike claude` command
as the closing action (floor `:76-80`). **This screen satisfies AC10's options-first
requirement** (`tui-page-requirements.md:139-140`).

**7-step indicator.** Steps 1–6 `◆` acid; step 7 renders `◈` **acid** (not ice) with
`glow`, labeled `"7 · ready"` — the one indicator glyph that carries
`text-shadow: var(--glow-acid)`.

**Layout.** Header line: "● setup complete" (acid, glow) + "your stack is configured."
(muted). Below it a two-column grid (`1fr 1.3fr`, collapsing to one scrolling column
below 860px):

- **Left — `Block title="capabilities" hotkey="c" tone="acid" focusable grow`**, data
  source = `GET /api/capabilities` verbatim (floor `:84`, same contract as Home's
  capability panel — never re-derive states locally): 6 `Cap` rows — capture, recall,
  memory, compression, weasel all `◆ ACTIVE` (acid); steering `◇ INERT` (rust) with fix
  hint "→ add an openrouter key in config to enable".
- **Right — `Block title="ways to run mike" hotkey="r" tone="acid" focusable grow`**,
  4 `Way` rows (5 with a real backend), static/config-derived, no live probe:
  1. **`mike claude`** (primary — acid + glow, left border acid) — "launch claude code
     through the proxy — the primary way to run mike."
  1. **`mike spawn open <folder> [options]`** — "launch a session into the `<backend>`
     backend, with a brief and the fleet protocol." Present only when the
     `MIKE_LAUNCH_BACKEND` the terminal step persisted (read from the process env on
     entering this step) is anything but `none`.
  2. **standalone** — `ANTHROPIC_BASE_URL=http://localhost:9988` — "with any
     anthropic-api client."
  3. **`mike-mcp`** — "query captured traffic from inside claude code."
  4. **browser ui** — `http://localhost:9990/`.
  - Close line (border-top): "press ⏎ to run `mike claude`."

**Keymap.** StatusBar: `[⏎ mike claude] [b browser ui] [? help] [q quit]` (no `[←]` —
terminal step). ShortcutsModal — global: `⇥` cycle panels · `⌃ /` this modal (no `j/k`,
no `⏎` select — both `Block`s are display panels here); ready: `⏎` run mike claude · `b`
open browser ui · `q` quit.

**States.** One state only — the completed/green screen. Capability rows reflect
whatever `GET /api/capabilities` returns at that moment (an Active/Inert mix other than
the one shown is the same component, different data — not a distinct visual state).

---

## Gaps vs the requirements floor (2026-07-21 review)

- **Step 2 — no in-flight probe state.** The floor calls for "live checks with results
  as they land" (`:56-57`); the mock shows only the fully-resolved end state (3 found, 1
  missing). The per-row in-progress indicator (pending → found/missing) has no mock —
  implement with directional motion, not a spinner (style guide §6 rules out pulsing).
- **Step 4 — execution states undesigned** (carried over from the prior spec): only the
  pre-execution path choice is designed. Running the chosen path (container pull / brew
  install / connection test) and the migration-with-progress step that follows have no
  mock, nor do success or failure-with-remediation states.
- **Step 5 — write-outcome states undesigned.** Only the pre-write consent screen is
  designed; there is no success confirmation (entry written) and no failure state (e.g.
  `~/.claude.json` unwritable), despite the floor's general "every executing step: …
  success, failure-with-remediation + retry/back" (`:86-88`). Also: the JSON entry shown
  (`"command": "mike-mcp", "args": []`) is illustrative — the floor requires the consent
  panel to show "exactly what will be written" (`:71`), so the implementation must
  render the actual resolved command/args, not this literal string, if they ever differ.
- **Step 6 — save-outcome states undesigned.** Only the unfilled/default-value state is
  designed. No validation-error state (e.g. a malformed embedding URL) and no
  save-confirmation state are designed, though values are written via `PATCH
  /api/settings/...` and could fail.
- **General — abandon-mid-flow has no visual state.** The floor's "abandoning mid-flow
  leaves nothing half-written" (`:87-88`) is a behavioral constraint (config only
  persists at step completion) with no distinct visual state on any of the seven screens.
