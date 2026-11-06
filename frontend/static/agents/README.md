# Agent identity art

Chopped from operator-provided character sheets. Two generations of source art exist,
and they are **not** interchangeable in size or richness:

- **Gen 1** (2026-07-12) — one multi-persona sheet covering all five personas at small
  scale. `forgemaster`, `architect`, `artisan`, `coordinator` still carry these crops:
  states 174x162, `icon.png` 82x82, `profile.png` 128x104.
- **Gen 2** (2026-07-13) — a dedicated single-persona sheet per agent, much larger and
  with per-state badges + profile variants. `documentor` is the first; the remaining four
  are expected to follow and should replace their gen-1 crops when they arrive.
  Source sheets are kept at `artifacts/<date>/<persona>-character-sheet.png`.

## Gen-2 file contract (documentor)

| file | size | what it is |
|---|---|---|
| `idle\|working\|done\|error\|blocked.png` | 214x336 | full state panel (portrait scene) |
| `icon-<state>.png` (5) | 248x248 | per-state circular badge |
| `icon.png` | 248x248 | persona badge — byte-identical to `icon-idle.png` on purpose, so consumers can use a uniform `icon-${state}.png` lookup *and* a stateless badge |
| `profile.png` | 156x184 | default bust |
| `profile-2\|3\|4.png` | ~150x184 | alternate busts (the sheet ships four; `-3` holds the OOK book) |

Gen-1 personas have only `icon.png`, `profile.png`, and the five state panels — no
per-state badges, no profile variants, and a landscape (not portrait) state aspect.
Any UI consuming both generations must not assume a shared aspect ratio.

## Persona → agent-type mapping

Personas are the art's own lineup, not the codebase's:

| persona     | agent-type          | token slot |
|-------------|---------------------|------------|
| forgemaster | rust-developer      | --agent-1  |
| architect   | frontend-developer  | --agent-2  |
| artisan     | ui-designer         | --agent-3  |
| coordinator | (coordinator)       | unslotted  |
| documentor  | technical-writer?   | unslotted  |

The sheet's last two personas (coordinator, documentor) differ from the token system's
slots 4/5 (`db-dev`, `watchdog`). Binding them to real agent-types — or minting new
slots — is an open decision (issues #176 / #178).

## State colour language

The gen-2 sheet ships its own pill/badge legend. Sampled mid-tones, for reference when
building state pills — these are the *art's* palette, not the app's OKLCH tokens, and the
two still have to be reconciled:

| state | sampled |
|---|---|
| idle | `#7D6E5B` |
| working | `#645D2D` |
| done | `#C98E34` |
| error | `#983921` |
| blocked | `#6A4D52` |
