---
thread: themeable-design-system
status: done
last_session: 2026-06-16 (session 1225a31b) @ cc566cf
---

> **2026-07-30:** the APCA/axe contrast gate this thread delivered was REMOVED on
> operator instruction — see [`.claude/rules/no-contrast-conformance-gating.md`](/.claude/rules/no-contrast-conformance-gating.md).
> The OKLCH token model, the 5 themes and the picker all remain. Do not rebuild the gate.

## State of play
**COMPLETE — rebased onto main, main's merged code migrated, all 5 themes green.**
Branch = themeable design system on top of current main (force-pushed, 88628cd). Full gate:
136/136 files, 693/693 tests, 0 failures (forge). Per-theme: forge/void/phosphor/crimson 693/693;
glacier 693/693 (1 flaky 40s timeout, passes on re-run). lint:tokens PASS all 5 themes.

Delivered end-to-end: two-layer OKLCH tokens + APCA gate (proven to bite); 653+ primitive refs →
roles; 5 themes (forge/glacier/void/phosphor/crimson) + production TopBar theme picker (no-flash,
persisted); categorical agent/data-viz ramps; honest gate coverage; docs rewritten. THEN rebased
onto main (45 commits squashed → 1; 9 conflicts resolved preserving main's homepage/steering
redesign + my system) and MIGRATED main's in-progress merged code (12 home zones, settings system,
steer-CTA, session detail) to roles + fixed its a11y (prohibited aria, role=grid overlay, opacity-
on-text dim, landmarks) so the whole branch passes the gate.

Known tooling flakiness (NOT code defects, pre-existing): intermittent @storybook/addon-svelte-csf
teardown $effect error can exit the runner non-zero though all tests pass; occasional vitest-browser
40s timeout on heavy-fixture stories (re-run passes). pre-rebase-themeable tag preserves old history.

Optional backlog (flagged): wire-up-or-remove 4 orphan components; collapse component stories to
single-state; delete legacy color aliases (dev/story refs); light theme.

## Phase tracking
- Phase 0: DONE (committed 82ddae3). Gate verified: lint:tokens PASS (26 APCA pairs),
  APCA canary 3/3, tokens:verify match, svelte-check clean.
- Phase 1: IN FLIGHT — 6 parallel batches (component/route SOURCE only, stories excluded;
  legacy aliases kept until end-of-phase sweep). Agents:
  A shell/ = aa92905614bcb4894 · B filter/ = a4bdd150a89015f4d · C prompts/ = aff0c34bd7eed5136 ·
  D message-view/ = aeeb7442009b8e0c9 · E live+top-level = ad034ac51d8f9e2d1 · F routes/ = adcd26b997d7899bd.
  Per-batch verify = headless (svelte-check + lint:tokens no-primitive + eslint).
  PHASE GATE (orchestrator, after all 6 merge): full `PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers
  npm run storybook:test`; `npm run lint:tokens --strict` (color-mix grep promoted to blocking);
  then delete legacy aliases in one sweep + re-run.

## Phase 1 result + reviewer punch list (2026-06-16)
All 6 batches landed & headless-verified. Gate 1 (lint:tokens): monotonic PASS, 26 APCA pairs
PASS, **No var(--primitive-*) in components/routes: PASS** (all 653+ refs gone). svelte-check 0
errors. color-mix backlog = 12 in stories (Phase 2) + 3 legit ROLE-token blends in steerers
routes (themeable, acceptable — NOT banning role-based color-mix). Full browser APCA/axe gate
running (baseline was 13/142 files failing).

PUNCH LIST for phase reviewer / operator:
1. **Categorical/identity colors have NO role token** (recurring; needs ui-designer + operator):
   - AGENT_COLORS 5-hue palette in shell/SessionOverview.svelte (orange #c87941, purple #9b72cf,
     blue, green, red) — left hardcoded.
   - #9a7bbf purple in live/ResourceGauges.svelte (data-viz tool_results series) — left hardcoded.
   - --violet agent-source chips in routes/agents — Batch F remapped to warning/success/info
     (semantically wrong: these are CATEGORIES, not statuses; revisit).
   DECISION NEEDED: add a themeable agent-identity / data-viz categorical ramp, or accept these
   stay non-themeable hardcoded hexes.
2. --interactive-active-glow referenced in spec §2e but NOT defined in §4 forge block (Batch B
   fell back to --interactive-active-border). Either define it or remove the reference.
3. teal→--data-teal for the "append" filter action (§3k literal) vs §2g (teal=data-viz only).
   Confirm intent or redirect append→--status-success.

## Phase 1 GATE verdict (2026-06-16, browser APCA/axe run)
Result: 11/143 story files fail axe (baseline was 13/142). Phase 0 text-ramp fix cleared the
foundation text failures (Typography, Spacing); Batch A cleared RequestBand. DIAGNOSIS (precise):
the migration is structurally CORRECT — no role fg value (green-11 #7ac78f, amber-11 #fcc394,
blue-11 #78bfd6, red-11 #eaa3a4, rose-11 #d1a9d2, teal-11 #83c3b1) appears in ANY axe failure;
every role-based element passes. The 11 failing files are exactly three things:
  1. ALPHA-ON-COLORED-TEXT (real bug): role colors dimmed by opacity/alpha below the floor
     (e.g. BigbroAttribution.svelte:393 opacity:0.75; rose-11 @ ~50% → #8e677c @ 3.4:1). Same
     defect class as the historical opacity:0.85 bug. FIX DISPATCHED (frontend-developer
     a473ff77506d30f22): full role fg for colored text; alpha only on fills/borders/glows;
     de-emphasis uses cleared --text-2/3, never an alpha'd status color. Affected: BigbroAttribution,
     shell/SteeringBand, shell/SessionSteeringSection, shell/RequestDrawer, live/SteeringMonitor,
     SteeringEnrolmentBadge, routes/+page.svelte.
  2. CATEGORICAL/IDENTITY colors with no role token. OPERATOR DECISION: add themeable categorical
     ramps. SPEC DISPATCHED (ui-designer addc2623fb099761e): --agent-1..N + --data-viz-1..N OKLCH
     ramps, contrast-cleared, [data-theme]-scoped; migrate AGENT_COLORS (shell/SessionOverview)
     + ResourceGauges #9a7bbf; add APCA pairs to lint. Component migration = follow-up FE task.
  3. Experimental theme stories (__foundations__/Themes 7, ColorExpansion 3) → Phase 3, deferred.

Phase 1 closes GREEN when: alpha fix re-gate passes + categorical ramps implemented & migrated +
re-run shows only the deferred Phase-3 theme-story failures remaining. THEN: legacy-alias deletion
sweep + final Phase 1 gate.

## Test-infra coverage gap (added to scope by operator, 2026-06-16)
The Storybook gate silently FAILS TO LOAD some stories → uncovered → gate lies about coverage.
Root cause: vitest.config.ts storybook-project alias passes through only `$lib/api/memory`; the
bare `$lib/api` stub prefix-matches any other `$lib/api/*` subpath (e.g. `$lib/api/steering`,
imported via SessionSteeringSection/InterventionRow) → rewrites to nonexistent path → crash.
src/lib/api/ now has memory.ts + steering.ts. FIX DISPATCHED (frontend-developer a45b10bdb3fdaee7e):
generic regex passthrough `$lib/api/* → real file` (array-form alias, order preserved) + full-gate
sweep to find & stub EVERY story that fails to LOAD, until all 143 collect. Bar: zero collection
errors; axe-assertion failures are fine (other phases). This makes the gate's coverage honest —
a prerequisite for trusting every later gate run.

## Alpha-fix re-dispatch (2026-06-16 18:04)
First alpha-on-text agent (a473ff77) STALLED — output frozen ~12:32, no edits on disk, no stall
notification. Re-dispatched fresh as frontend-developer a599b1bd249f0c8a3 (same scope: 7 files,
opacity/alpha-on-colored-text → full role fg or cleared text role). Honest gate state pre-fix:
6/143 files fail (18 axe + 3 pre-existing interaction timeouts). After this lands: legacy-alias
deletion sweep → final Phase 1 gate.

## Phase 1 close + alias-sweep RESCOPED (2026-06-16 18:xx)
Alpha-on-text fix confirmed already in committed tree (first agent's edits were swept into a
`git add -A` commit; re-dispatched agent correctly verified ground truth, made no redundant edit,
scoped gate = 0 color-contrast failures in the 7 files). Independent closing gate running.

ALIAS-SWEEP RESCOPED after inspecting app.css + ref counts:
- Legacy COLOR aliases (--green/--amber/--rose/--teal + -dim/-border): 0 COMPONENT refs, but some
  STORIES still use them in color-mix (SessionOverview/TurnBracket/BandGroup .stories). → DELETE
  AFTER Phase 2 (story cleanup), not now, else those stories break.
- Surface/border NAME-aliases (--bg/--surface/--sidebar/--border-color/--border): 331 refs across
  30+ files, ALL resolve to ROLE tokens (themeable). --surface-2/--border-subtle are ROLES inside
  the [data-theme=forge] block (overridable), NOT primitive leaks. → KEEP as permanent documented
  role-aliases; migrating 331 refs for cosmetic single-naming isn't worth the regression risk.
CONCLUSION: Phase 1 core objective (component/route source off primitives → roles) DONE. Theming
is unblocked — every surface/border/status token re-themes via [data-theme]. Move to Phase 2.

## Phase 2 underway (2026-06-16) + a11y fold-in
Plan: artifacts/2026-06-16/phase2-plan.md. Operator: fold a11y fixes in (gate fully green);
orphans = KEEP ALL 4, flag only (no component deletion). Two agents running:
- a11y fix (frontend-developer a4ed9c496ed5fe9a4): BigbroAttribution scrollable-region-focusable,
  SteeringMonitor + agents/[agentId] nested-interactive/aria, RequestDrawer timeout.
- Phase 2 stories (frontend-developer ad5ae60979bbf542f): author 3 page stories (steering,
  steerers/[id], filter/workbench) single full-state; delete ui/ sub-part fragment stories (keep
  one top-level story per primitive); empty meta-test KNOWN_GAPS.
Backlog (flagged, not done): wire-up-or-remove HealthStrip, ThroughputKpis, OutcomeBadge,
SteeringEnrolmentBadge; normalize 77 component stories to single-state (deferred — coverage risk);
color-alias deletion after stories stop referencing --green etc.; Phase 3 themes + theme switcher.

## Phase 2 DONE + gate-close cleanup (2026-06-16)
Phase 2 committed (244fdfd): 3 page stories authored, 27 ui fragment stories deleted (7 primitive
top-level kept), KNOWN_GAPS emptied. Consolidated gate: 5 failed / 114 passed (119 files; was 143).
ALL 260 color-contrast mentions are in Themes.stories + ColorExpansion.stories (experimental themes,
Phase 3) — STILL zero production color-contrast. The other 3 failing files are non-color:
- filter/workbench page story: nested-interactive(24)+aria(18) from bits-ui WorkbenchRuleCard.
- RequestDrawer: OpenRequest 15s timeout (story fixture missing /deltas stub).
- agents/[agentId]: SaveError/DeleteError play-function timeouts.
Gate-close cleanup DISPATCHED (frontend-developer a5044d77151fabae4): fix the bits-ui
nested-interactive/aria, add RequestDrawer /deltas stub, fix agents error-state play functions.
Target: gate green except Themes/ColorExpansion (Phase 3). Then color-alias deletion + Phase 3 themes.

## Phase 5: REBASE onto main + migrate main's merged code (2026-06-16, operator: "rebase" + "migrate main's code too")
Rebased 45 commits -> 1 squashed commit (acade96) onto origin/main (89f039f); force-pushed. 9
conflicts resolved (TopBar=main slim bar+my picker; analysis=main href+my ring; steering area=
accept main's steerers->steering move, drop my superseded steering stories + NewMessagesAffordance;
SessionOverview/SessionTimeline=take main's deleted-dead-CSS side; SteererFleet rename). svelte-check
0, lint:tokens PASS (all 5 themes survived). pre-rebase state tagged pre-rebase-themeable.

Post-rebase gate: my work GREEN; main's IN-PROGRESS merged code FAILS the stricter gate (contrast +
heavy aria-*). Operator chose: MIGRATE MAIN'S CODE TOO. Universe = 12 home/ zones, 4 settings/ +
settings page, new routes (sessions/tools/steering[id]/steering-concepts), DestinationNav, SteererFleet,
+ modified shell. Agents running: home zones = aff71d528aa00223d; settings = af1ae6de6b2df623b.
Fresh full gate (bhef5a7vj) computing complete failing list -> will dispatch remaining batches (routes/nav,
modified shell) from it. Target: full gate green across all themes, then re-push.

## Session log
- 2026-06-16 (1225a31b) @ cc566cf — Dispatched ui-designer (palette/themes first pass), then a
  research team: permanent contrast fix (OKLCH+APCA), Storybook gate+theming architecture, frontend
  audit (reconstructed after the Explore agent stalled). Identified root cause: 653+ primitive-token
  refs, no semantic-role layer — same defect behind recurring contrast patches. Operator chose a
  phased parallel-agent workflow. Wrote workflow.md; installed node_modules; began Phase 0.
- 2026-06-16 (1225a31b) — Completed Phases 0-4 + theme picker. 5 themes, APCA gate, picker, docs. Gate 119/119 green across all themes.
- 2026-06-16 (1225a31b) — Rebased onto main + migrated main's in-progress merged code (home/settings/steering). Full gate 136/136; all 5 themes 693/693. Force-pushed 88628cd.
