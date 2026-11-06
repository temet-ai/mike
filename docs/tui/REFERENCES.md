---
title: TUI References
status: active
created: 2026-07-23
updated: 2026-07-24
summary: References behind portable TUI Design, plus the Rust libraries used by the mike application.
---

# TUI References

Everything referenced by the source article behind portable, application-neutral **TUI
Design**, plus the Rust libraries used by the mike application example. Links marked
*(named, unlinked)* are canonical homes for things the article names without linking.

**Source article:** Stefanie Jane, *"The Terminal Renaissance: Designing Beautiful TUIs in
the Age of AI"* — <https://hyperbliss.tech/blog/2026.04.04_terminal-renaissance/>

## Portable TUI Design stack

TUI Design targets Ratatui. In this repository, mike is the application example using this
Rust stack:

| Library | What for | Link |
|---|---|---|
| **Ratatui** | The TUI framework — immediate-mode rendering, the article's pick for dashboard/monitor apps ("runs beautifully over SSH; Netflix, AWS, and OpenAI ship Ratatui apps") | <https://ratatui.rs/> |
| **crossterm** | Ratatui's default terminal backend (events, raw mode, colors) *(named via the Ratatui stack)* | <https://github.com/crossterm-rs/crossterm> |
| **Opaline** | Token-based theme engine for Ratatui — implements the palette → tokens → styles → gradients pipeline (TUI-C5) and ships the five SilkCircuit variants among 20 builtin themes, contract-tested, user themes as TOML, runtime switching | <https://github.com/hyperb1iss/opaline> |
| **SilkCircuit** | The adopted design language — palettes for every variant (`lua/silkcircuit/variants.lua` is the token source of truth for TUI-C4a) | <https://github.com/hyperb1iss/silkcircuit> |
| **ghostty-automator** | Emulator-level IPC exposing the true rendered screen (every cell's char, colors, attrs, cursor) over a Unix socket — the verification loop's eyes (TUI-V1) | <https://github.com/hyperb1iss/ghostty-automator> |
| **ghostty-automator-python** | Playwright-style async API over the IPC: `send`, `wait_for_text`, `screen`, `cells`, `screenshot`, `press`, `click`, `expect` | <https://github.com/hyperb1iss/ghostty-automator-python> |
| **ghostty-terminal-automation skill** | Agent skill wrapping the whole thing (`npx skills add hyperb1iss/ghostty-automator-python`) | <https://github.com/hyperb1iss/ghostty-automator-python/tree/main/skills/ghostty-terminal-automation> |
| **ChromaCat** | Terminal output colorization with gradient patterns, 40+ themes — optional, for CLI-face output flair | <https://github.com/hyperb1iss/chromacat> |

Other frameworks the article covers (not the adopted stack; kept for the patterns that
transfer — unidirectional data flow, vim keys, help overlays, semantic color):

- **Ink** (TypeScript/React for terminals; Claude Code is built on it) — <https://github.com/vadimdemedes/ink>
- **Bubbletea** (Go, Elm architecture) — <https://github.com/charmbracelet/bubbletea>
- **Textual** (Python, CSS-like styling) — <https://github.com/Textualize/textual>

## The author's production apps (the article's evidence base)

- **Unifly** — UniFi network dashboard, Ratatui; dual-product pattern (CLI + TUI from one core) — <https://github.com/hyperb1iss/unifly>
- **Iris Studio / Git-Iris** — six-mode AI git workflow, Ratatui; pure-reducer state, universal chat overlay — <https://github.com/hyperb1iss/git-iris>
- **q** — minimal Claude Code CLI (TypeScript/Bun/Ink); pipe mode = know when not to be a TUI — <https://github.com/hyperb1iss/q>
- **Vigil** — PR lifecycle manager with agent dispatch; state-machine visual language, HITL/YOLO spectrum — <https://github.com/hyperb1iss/vigil>
- **hyperskills** — the author's agent skills, including the ~3,000-line `tui-design` knowledge base and the 23-exemplar TUI study — <https://github.com/hyperb1iss/hyperskills>

## Terminal emulators

- **Ghostty** — GPU-accelerated, the automation target — <https://ghostty.org/>
- **Kitty** — inline image protocol *(named, unlinked)* — <https://sw.kovidgoyal.net/kitty/>
- **WezTerm** — built-in multiplexer *(named, unlinked)* — <https://wezterm.org/>
- **Rio** — WebGPU renderer *(named, unlinked)* — <https://raphamorim.io/rio/>

## Exemplar TUIs the article studies

*(named, unlinked — canonical homes)*

- **lazygit** (persistent multi-panel) — <https://github.com/jesseduffield/lazygit>
- **btop** (multi-panel / widget dashboard) — <https://github.com/aristocratos/btop>
- **bottom** (widget dashboard) — <https://github.com/ClementTsang/bottom>
- **yazi** (Miller columns) — <https://github.com/sxyazi/yazi>
- **ranger** (Miller columns) — <https://github.com/ranger/ranger>
- **k9s** (drill-down stack) — <https://github.com/derailed/k9s>
- **harlequin** (IDE three-panel) — <https://github.com/tconbeer/harlequin>
- **fzf** (overlay/popup) — <https://github.com/junegunn/fzf>
- **atuin** (overlay/popup) — <https://github.com/atuinsh/atuin>
- **htop** (header + scrollable list) — <https://github.com/htop-dev/htop>
- **tig** (header + scrollable list) — <https://github.com/jonas/tig>

## Modern CLI tooling the article cites as the raised floor

*(named, unlinked — canonical homes)*

- **ripgrep** — <https://github.com/BurntSushi/ripgrep>
- **bat** — <https://github.com/sharkdp/bat>
- **eza** — <https://github.com/eza-community/eza>
- **fd** — <https://github.com/sharkdp/fd>
- **zoxide** — <https://github.com/ajeetdsouza/zoxide>
- **OpenCode** (terminal-native AI coding agent) — <https://github.com/sst/opencode>

## Design guides and prior art

- **clig.dev** — CLI design guide (explicitly excludes TUIs: the gap the article fills) — <https://clig.dev/> · the exclusion: <https://clig.dev/#introduction>
- **Base16 / Tinted Theming** — color-only theming system, 230+ palettes *(named, unlinked)* — <https://github.com/tinted-theming>
- **awesome-tuis** — most-starred TUI catalogue, zero design resources *(named, unlinked)* — <https://github.com/rothgar/awesome-tuis>
- The article also cites a 1983 ACM paper as essentially the only academic work on terminal
  interface design (no link given in the article).

## Context and further reading

- *Claude Code is the Inflection Point* — SemiAnalysis — <https://newsletter.semianalysis.com/p/claude-code-is-the-inflection-point>
- JetBrains Developer Ecosystem 2025 (the "69% keep a terminal open" stat) — <https://devecosystem-2025.jetbrains.com/>
- *Terminals, agents, and the control plane nobody built* — Adel Zaalouk — <https://adelzaalouk.me/2026/feb/22/terminals-agents-and-the-control-plane-nobody-built/>
- *AI coding tools are shifting to the terminal* — TechCrunch — <https://techcrunch.com/2025/07/15/ai-coding-tools-are-shifting-to-a-surprising-place-the-terminal/>
- *Building a TUI Is Easy Now* — Hatchet — <https://hatchet.run/blog/tuis-are-easy-now>
- *Learning From Terminals to Design the Future of User Interfaces* — Brandur — <https://brandur.org/interfaces>
- *TUI Design* — Jens Roemer — <https://jensroemer.com/writing/tui-design/>
- Claude Code PTY limitation the automation story responds to — <https://github.com/anthropics/claude-code/issues/9881>
