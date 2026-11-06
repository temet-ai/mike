---
title: mike TUI — Setup screen spec
status: active
created: 2026-07-20
updated: 2026-11-06
summary: Layout, data sources, states, and keymap for all ten steps of the guided Setup flow — the terminal step chooses and live-validates the backend `mike spawn` launches into.
---

# Setup (guided flow)

The steps below are the ones `all()` in
`crates/mike/src/tui/screens/setup/steps/mod.rs` returns, in that order; each is a
`WizardStep` (`setup/step.rs`) with a `names()` entry that `mike setup <name>` opens
directly and a `label()` the step indicator shows. The original mocks (one per step of
an earlier six-step design) are not in this repository and no longer match; this spec
describes the implementation. Style rules: see [`../STYLE_GUIDE.md`](../STYLE_GUIDE.md).

| Step | `mike setup <name>` | Label | What it does |
|---|---|---|---|
| 1 | `welcome` | welcome | Says what mike records and where it stays; asks whether setup may look at this machine. |
| 2 | `machine` | machine | Lists the tools found on this machine with their versions, and the ones missing. |
| 3 | `database` | database | Chooses where captures are stored: Docker, Homebrew, your package manager, an existing PostgreSQL, or memory only. |
| 4 | `service` | service | Installs the background service (launchd or systemd) that runs the proxy and the sidecar. |
| 5 | `claude` | claude code | Registers `mike-mcp`, checks the sidecar route, writes a shell function, installs the mods, records the folder-trust choice. |
| 6 | `features` | features | Chooses a feature set: watch-only, recommended, or everything. |
| 7 | `models` | models | Chooses the provider for the model-calling features and sets the spend limits. |
| 8 | `embeddings` | search | Switches on search by meaning through Ollama or an OpenAI-shaped embedding server. |
| 9 | `terminal` | terminal | Chooses and live-validates the backend `mike spawn` launches into. |
| 10 | `ready` | ready | Says how to start Claude Code and waits for the first captured request. |

`steps/proxy.rs` is not a step: it is `ensure_proxy`, the helper that starts the proxy
when a step needs it and nothing answers on the inspect address (and says so instead
when the sandbox switch is set).

## Purpose

From nothing to a green system without reading docs. First run (no config found) lands
here automatically; re-enterable any time from the Home menu, and any one step again
with `mike setup <name>`.

## Entry

No config found on launch, explicit navigation from Home's `menu` → Setup, or `mike
setup [<name>]`.

## Shared chrome

Every step shares one header (`setup/render.rs`): the brand, a `setup` crumb, and one
mark per step — completed steps render the filled diamond in acid, the current step
renders the current-step glyph in ice (acid on the last step), upcoming steps render the
open circle in `--fg-dim`. A step at or before the current one shows `N·label`; later
ones show only the mark. The status bar reads `step N/10 · <label>` (or the step's
`progress()` tail, or its `banner()` when it has one) and the step's own `hints()`.
The shared key hints, which every step draws from, are: `↑↓ / j k` choose · `⏎` run the
command shown, or continue · `e` type into the field of the chosen option · `r` check
again · `s` skip the step · `←` back a step · `f` finish later · `Esc` back to home ·
`q` quit app.

Most steps are one `Panel` (`setup/panel.rs`): findings at the top (`●` ok in acid, `◇` note in ice, `○` failed in red —
`crate::finding::Finding`, drawn by `panel.rs`), a list of choices with a one-line description each,
and a note underneath. A choice that runs something shows the command it will run, and
`⏎` runs it.

## Screen accent

`ice` (configure) for steps 1–9; the last step's current-step mark is `acid`
(liveness/success).

---

## Step 1 — Welcome

**Purpose.** What mike records and where it stays, before anything is written. The
note says that mike captures every request and response in full, stores them in a
PostgreSQL database on this machine, sends nothing off it, and refuses web pages on the
proxy port; and that `mike setup <step>` runs any one step again later.

**Choices.** "Look at this machine" (recommended) — setup lists the tools it can use
that are on this machine and offers only choices that work here; "Do not look" — each
step then checks only the tools it needs when it runs.

**Tests.** `setup/tests/welcome_machine_tests.rs`.

---

## Step 2 — Machine

**Purpose.** The dependency probe. With "Look at this machine" chosen, one row per tool
found, with its version; a note names what is missing. With "Do not look" chosen the
panel says so and each later step checks only the tools it needs. Missing items narrow
later choices; they never block the flow.

**Tests.** `setup/tests/welcome_machine_tests.rs`.

---

## Step 3 — Database

**Purpose.** Where captures are stored, as a genuine choice with the trade-off of each,
none hidden as "advanced". The finding at the top says what is configured now
(`database_url is set`, `persist_enabled is false: captures are kept in memory only`,
or `No database is configured yet`).

**Choices** (`steps/database.rs`), each with the command it runs shown in the panel:

| Choice | Description |
|---|---|
| Run PostgreSQL in Docker | Starts PostgreSQL 19 with pgvector in a container on 127.0.0.1, keeps the data in a Docker volume, generates a password into the mike config file and applies mike's migrations. |
| Install PostgreSQL 19 and pgvector with Homebrew | Runs Homebrew commands that need no root, then creates the mike database and its three extensions. |
| Install PostgreSQL with your package manager | mike prints the commands for this machine and does not run them; run them, then choose the existing-database option with the URL they end with. |
| Keep the configured database | Connects to it and checks the PostgreSQL version, the three extensions and the migrations. |
| Use a PostgreSQL I already run | Checks the URL you type (`e` edits it), and saves it only when the version, the extensions and the migrations pass (`mike db check <url> --save`). |
| Keep captures in memory only | No database; captures die with the process. |

**Tests.** `setup/tests/database_tests.rs`.

---

## Step 4 — Service

**Purpose.** A proxy that is always up. The finding says whether a mike proxy answers on
the inspect address. "Install the background service" writes two units (`mike.service`
and `mike-forward.service` under systemd, `ai.temet.mike.proxy` and
`ai.temet.mike.forward` under launchd) that run this mike binary, start at login and
restart after a crash, then waits until the proxy and the sidecar answer. It refuses
when something already answers on mike's ports, says so on a system that is neither
macOS nor Linux, and does nothing while the developer stack's own units are running,
because a service would fight them for the same ports and database.

**Tests.** `setup/tests/service_tests.rs`.

---

## Step 5 — Claude Code

**Purpose.** Everything Claude Code needs to run through mike, as five parts
(`steps/claude/`) that `⏎` and `←` walk in order inside one step:

1. **MCP** (`mcp.rs`) — adds a `mike` entry under `mcpServers` in `~/.claude.json` with
   the absolute path of `mike-mcp` and `MIKE_INSPECT_URL`, keeps every other key in that
   file, then starts `mike-mcp` once and counts the tools it lists. Skipped: Claude Code
   has no mike tools until `mike claude-code mcp` registers them.
2. **Route** (`route.rs`) — checks that the forwarding sidecar answers and that its
   certificate-authority file exists.
3. **Shell** (`shell.rs`) — writes a function into the startup file of your login shell
   (replaced in place when run again) that starts Claude Code through mike with the
   chosen route; or leave it and start Claude Code with `mike claude`.
4. **Mods** (`mods.rs`) — installs the Claude Code mods (panes and status line) from the
   folder beside this mike binary, in a checkout, or under the Homebrew prefix. Skipped:
   `mike claude-code mods` installs them later.
5. **Trust** (`trust.rs`) — whether `mike claude` marks a folder as trusted or leaves
   that to Claude Code, which asks once per folder.

**Tests.** `steps/claude/tests.rs`.

---

## Step 6 — Features

**Purpose.** Which switches are on. The finding says whether a proxy answers (a choice
made while nothing answers is saved and applied when the proxy next starts) and that
switch settings are kept in the database — in memory-only mode the proxy keeps none
and everything stays at its default.

**Choices** (`steps/features.rs`), each the `mike features <set>` command it runs:

| Choice | Description |
|---|---|
| watch-only | mike records and shows the traffic and changes none of it. Every switch that rewrites a request or sends captured text to another service is off; the rest keep their defaults. |
| recommended | Watch only, plus recall, lessons and reminders — notes added to what Claude Code sends to Anthropic, built on this machine; nothing leaves it for them. |
| everything | All switches on the settings page, including the ones that send captured text to OpenRouter (steerers, turn judge, session judge, lesson proposer, turn guard) and the ones that rewrite requests and responses. The ones that call a model do nothing until a provider is set up in the next steps. |

**Tests.** `setup/tests/features_tests.rs`.

---

## Step 7 — Models

**Purpose.** The provider behind the features that call a model. Shown only when the
chosen features run a background model job and the proxy is running; otherwise the
panel says so and `mike setup models` opens it again later.

**Choices** (`steps/models/intro.rs`): run each call with `claude -p` under the Claude
Code login on this machine (no key, your own Claude plan); with `codex exec` under the
ChatGPT login (no key, your own plan); or call OpenRouter with your key, which also
reaches Claude and GPT models. For OpenRouter, type the name of an environment variable
that holds the key and only the name is saved, or paste the key and it is saved in the
owner-only config file. The form that follows (`steps/models/build.rs`) sets the daily
spend limit in US dollars, the most calls in the rolling window and the calls-per-minute
limit; Claude and Codex calls use your own plan, so the limits count OpenRouter calls
only.

**Tests.** `steps/models/tests/`.

---

## Step 8 — Search (embeddings)

**Purpose.** Search by meaning. The finding says whether the Ollama server answers on
`127.0.0.1:11434` and whether its model is pulled.

**Choices** (`steps/embeddings/plan.rs`): pull the model through Ollama (about 270 MB,
skipped when it is already there), set `embedding.url`, `embedding.model` and
`embedding.enabled` in the running proxy and fetch one embedding to check it — the text
being embedded stays on this machine; install Ollama as a Homebrew service first when it
is missing (or, without Homebrew, mike prints the install command and does not run it,
because it needs root); use any server with an OpenAI-shaped `/embeddings` route that
returns 768 numbers per text; or leave search by meaning as it is — recall uses text
matching while it is off.

**Tests.** `steps/embeddings/tests.rs`.

---

## Step 9 — Terminal

**Purpose.** Choose the terminal backend `mike spawn` launches Claude Code sessions into,
as a genuine choice with one-line trade-offs, and prove it live before saving. Setup
never saves a backend that failed validation; `none` is always available and says what
it costs.

**Layout.** One `Panel` like the other steps (`plan_panel()` in
`steps/terminal/plan.rs`). On entry the step refreshes the facts for `herdr`, `tmux`,
`hyprctl`, `xdg-terminal-exec` and `brew`, then runs `probe()` on every installed
backend with an adapter, all at once, each bounded by its own subprocess timeouts. The
findings at the top name each tool found (with its version) and, per probed backend,
`<id> is ready` or `<id> is not ready: <first failed check's remedy>`; when nothing is
found: "No terminal tool was found: herdr, tmux and hyprctl with xdg-terminal-exec are
all absent".

The choices are, in order: "Install herdr" (recommended, `brew install herdr` when
Homebrew is present) when herdr is missing, then one choice per installed backend
(`installed()`: `herdr` when herdr is on PATH, `tmux` when tmux is, `terminal` when both
`hyprctl` and `xdg-terminal-exec` are, and always `none`), the configured one first and
marked `(current)`. Each choice shows its trade-off (`trade_off()`), how it is saved,
and the command it runs, `MIKE_LAUNCH_BACKEND=<id> mike spawn doctor`:

| Backend | Trade-off (`trade_off()`) | Saved how |
|---|---|---|
| herdr (recommended) | "A persistent server with agent states and a session-uuid join; the fleet's first backend." | after a live check |
| tmux | "For headless hosts, ssh and containers: no display needed, no agent state." | after a live check |
| terminal | "Plain windows through xdg-terminal-exec and hyprctl: window placement only, no agent state." | after a live check |
| none | "Launch by hand: `mike claude` works, `mike spawn` refuses and the fleet protocol is unavailable." | without a live check |

`trade_off()` also has lines for `herdr-core` ("Tabs in the Herdr desktop app through
herdr-core (`hc`), with agent states.") and `ghostty` ("Ghostty windows on macOS through
AppleScript."), but `installed()` never lists them, so the wizard does not offer either;
`ghostty` has no adapter in this build (`backend_for` is `None`).

**Data / engines beyond this screen.** `probe()` and `validate()` on
`launch::TerminalBackend` (the herdr adapter opens a labelled scratch workspace, runs a
marker command, reads it back, lists agents and closes the workspace). A passed
validation lands in the mike config file as `launch_backend = <id>` through
`persist_env_var_at` (`setup/persist.rs`), mirrored into the process env. The
validation report is not written to the database from here; the first `mike spawn
doctor` records it in `launch.backend_validations`.

**Keymap.** The shared hints. `⏎` on:
- `none`: saves `launch_backend = none`, no validation.
- a backend: probes it again; a failed probe shows its checks and saves nothing. A
  passing probe runs `validate()`, shows one finding per check in the report's order,
  never trimmed (`●` passed, `○` failed with its remedy), then saves on a pass or ends
  with "The live check failed, so nothing was saved. Fix it and run the check again, or
  choose another backend."
- "Install herdr": runs the install, then re-enters the step.
- `r` re-enters the step: facts and probes are refreshed and the findings cleared.

**States.** probed (ready / not ready per backend) · validation failed (checks shown,
step unchanged) · validation passed (config written). Each has a render test in
`crates/mike/src/tui/screens/setup/steps/terminal/tests.rs`; the flow across steps is in
`crates/mike/src/tui/screens/setup/tests/`.

**Gaps.** Checks land as one report after `validate()` returns rather than one by one as
they complete, and the event loop draws only after the awaited call returns — the same
in-place await the other steps use. No per-check progress.

---

## Step 10 — Ready

**Purpose.** The closing step. It says how to start Claude Code — with `mike claude`,
or with the function step 5 wrote into your shell (open a new terminal first if it is
not found) — then waits for the first request from Claude Code and, when it arrives,
prints it: when it arrived, method and path, model, session and project. `c` stops
waiting; `r` checks again and waits again.

**Tests.** `steps/ready/tests.rs`.

---

## Gaps

- **No mocks.** The earlier six-screen design is not in this repository and no longer
  matches the ten steps; the implementation is the source of truth.
- **Abandon-mid-flow has no visual state.** Config persists at step completion, so
  leaving mid-flow writes nothing half-done, but no screen shows that.
- **Step 9** — see its own Gaps above.
