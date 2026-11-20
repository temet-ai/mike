---
title: Themeable Design System — Phased Parallel Workflow
status: active
created: 2026-06-16
updated: 2026-07-30
summary: The phase plan, agent-team structure, and reviewer→fix gates for rebuilding the design system on a two-layer OKLCH token model with user-selectable themes. The APCA contrast gate it also describes was removed 2026-07-30 and must not be restored.
---

> **The contrast gating described below is DEAD as of 2026-07-30.** The APCA `Lc` floors, the
> axe WCAG-2 contrast rules, and every gate that enforced them were removed on operator
> instruction — see [`.claude/rules/no-contrast-conformance-gating.md`](/.claude/rules/no-contrast-conformance-gating.md). Do NOT implement, restore, or cite the
> contrast assertions in this document. Everything else here (the two-layer OKLCH token model,
> the ramps, the theme architecture) still stands.

# Themeable Design System — Workflow

## Goal state (definition of done for the whole program)

1. **Two-layer token model** in `app.css`: primitive OKLCH ramps → semantic role tokens
   (text/surface/border/status/interactive), roles scoped under `[data-theme="…"]`.
2. **Components reference ONLY semantic role tokens** — zero `var(--green|amber|blue|red|…)`
   primitive refs in component/route source; zero per-selector `color-mix` contrast hacks.
3. **Contrast true-by-construction**: APCA `Lc` assertion (Lc 75 body / 60 secondary+badge)
   layered on the existing axe WCAG-2 floor, run **per theme** in the `storybook:test` vitest gate.
   A disallowed text/surface pair fails the build.
4. **Storybook**: one full-state story per route (25 pages); primitive/fragment stories pruned;
   `__foundations__` token/theme reference stories kept; live theme switcher in the toolbar.
5. **Multiple user-selectable themes**, each APCA-passing across every page story.

## Decisions locked (operator, 2026-06-16)

- Sequencing: **phased + parallel agent teams, reviewer→fix gate between every phase.**
- Contrast: **APCA Lc primary + keep axe WCAG-2 floor.**
- Stories: **page-primary; delete primitive/fragment stories; keep foundation stories.**

## Coordination model

The orchestrator (main session) is the standing coordinator. Per phase it: dispatches the
parallel worker team → runs a dedicated reviewer agent (the gate) → drives a fix loop on the
same workers until the gate is green → only then opens the next phase. No phase starts on an
unverified predecessor. Flat fleet (no nested coordinator-agents) for reliability.

**Verification is mandatory at each gate** (CLAUDE.md): captured command output with 0 failures,
or a live Storybook check, or an explicit `Blocked:` declaration. `node_modules` must be present
for svelte-check / `storybook:test` to run.

## Phases

### Phase 0 — Foundation (token contract + gate infra) — SERIAL, blocks all
Team: `ui-designer` (design) → `frontend-developer` (impl). 
- ui-designer: semantic role taxonomy + OKLCH primitive ramps + **primitive→role mapping
  dictionary** (the migration key Phase 1 consumes) + fix the non-monotonic text scale.
  Source inputs: `artifacts/2026-06-16/contrast-permanent-fix-research.md`.
- frontend-developer: implement two-layer `app.css` (`[data-theme="forge"]` default); APCA
  helper (`apca-w3`); wire it into the storybook vitest gate; `@storybook/addon-themes`
  data-attribute switcher; meta-test failing when a route lacks a page story.
  Source inputs: `artifacts/2026-06-16/storybook-contrast-and-theming-research.md`.
- **GATE (reviewer):** scale monotonic ✓; APCA helper fails a deliberately-bad pair ✓;
  theme swap cascades on a sample component ✓; existing stories green under new gate ✓.

### Phase 1 — Semantic-role migration — PARALLEL (the 650-site rewire)
Team: ~6 `frontend-developer` agents, one per batch, all consuming the Phase-0 mapping dictionary.
- Batch A: `shell/` · Batch B: `filter/` · Batch C: `prompts/` + `routes/prompts` ·
  Batch D: `message-view/` · Batch E: `live/` + top-level components ·
  Batch F: `routes/` pages (steerers, analysis, diagnostics, agents, help).
- Each: re-point primitive refs → semantic roles; delete inline color-mix hacks; fix raw hex.
- **GATE (reviewer, per batch):** zero primitive refs remain (grep) ✓; APCA+axe gate green ✓;
  visual parity in Storybook ✓. Fix loop until clean.

### Phase 2 — Storybook restructure + prune — PARALLEL
Team: `ui-designer` (authors page stories) + `frontend-developer` (deletes/transitive-usage).
- One full-state story per route; delete 35+ primitive sub-part stories + redundant multi-state
  stories; orphan-component deletion **only after** a transitive-usage pass proves unreachable.
- **GATE:** every route has exactly one full-state story; theme switcher works on all; meta-test green.

### Phase 3 — Build themes — PARALLEL per theme
Team: `ui-designer` per theme. Author each as `[data-theme="x"]` role overrides; APCA-pass across
all page stories. **GATE:** APCA gate green for every theme × every page story.

### Phase 4 — Final integration gate
`npm run check` (svelte-check + storybook:test), eslint, `scripts/fidelity-check`, full review,
`technical-writer` rewrites `docs/frontend/STYLE_GUIDE.html` + `tokens.md` to the new model.

## Inputs (read before any phase)
- `artifacts/2026-06-16/contrast-permanent-fix-research.md` (OKLCH + APCA structural fix)
- `artifacts/2026-06-16/storybook-contrast-and-theming-research.md` (gate + token architecture)
- `artifacts/2026-06-16/frontend-audit.md` (pages, stories, 653 primitive-ref hot spots)
- `.claude/rules/inspection-fidelity.md` (no truncation — binds all design work)
