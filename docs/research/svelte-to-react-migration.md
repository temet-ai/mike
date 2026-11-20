---
title: "Frontend Framework Migration: Svelte → React — Decision Report"
status: active
created: 2026-06-21
updated: 2026-06-21
summary: Decision-grade investigation into porting the temet-proxy SvelteKit SPA to React/Next.js — effort, gains/losses, ecosystem unlocks, and framework choice. Verdict: Conditional No-Go; if pursued, React 19 + Vite (not Next.js).
session_id:
  - b00952ce-0019-46d8-b9cf-a7a27dd8e0b6
---

# Svelte-to-React Migration: Decision Report

## Executive Summary

**Verdict: Conditional No-Go.** The migration is technically sound and the React ecosystem is a plausible long-term home — but the effort is 36–61 person-weeks solo (16–35 calendar weeks with a team), the tool is purely operator-facing on localhost, and most claimed React ecosystem wins are available in Svelte today (see §3). Migration is justified only if the motivation is ecosystem longevity and hiring over a multi-year horizon, and the operator accepts a multi-month feature window per phase.

**If the migration proceeds, the target is React 19 + Vite (not Next.js).** Next.js is rejected on architecture: dynamic IDs not enumerable at build time, live WebSocket data, no public users — every Next.js differentiator is irrelevant (see §4 for the full argument). Full recommended stack: §4.

**Migration strategy: incremental strangler-fig only.** Big-bang rewrite is operationally unacceptable for 181 components with an actively used tool. Mandate a 1-week calibration spike on `/sessions/[sessionId]` before committing to the full plan.

**Headline numbers:**
- Rewrite surface: ~330 files (181 Svelte components + 25 rune-state modules + 185 stories + ~20 infra files)
- Framework-agnostic assets that port unchanged: ~100 TS files, all Tailwind CSS, all idb/utility packages
- Effort: **36–61 person-weeks solo** / **16–35 calendar weeks with 2–3 engineers** (phase breakdown: §1)
- Highest-risk phase: state layer — 25 rune singletons require design review per module, not mechanical translation (see §2)

---

## 1. What It Would Take

### Svelte-Coupled Surface: Equivalence Table

| Svelte / SvelteKit surface | Count / scope | React SPA equivalent | Next.js equivalent | Effort (1–5) |
|---|---|---|---|---|
| `.svelte` component files (rune syntax, template, `bind:`) | 181 files | `.tsx` React components + hooks | Same (RSC/client boundary audit added) | 4 |
| `.svelte.ts` rune-state singletons (`$state`, `$derived`, `$effect` at module scope) | 25 files | Zustand v5 store slices (one per singleton) | Same + server/client split concern | 5 |
| `bits-ui` headless primitives (Dialog, Tooltip, Popover, Tabs, Separator) | 68 `ui/*.svelte` wrappers | shadcn/ui + Radix UI / Base UI primitives | Same | 3 |
| `@lucide/svelte` icons | 76 import sites | `lucide-react` (drop-in, same tree-shaking) | Same | 1 |
| SvelteKit file-based routing + `+page.ts` loaders | 14 route groups, 3 load functions | TanStack Router v1 (typed params, loaders) | App Router with `'use client'` on ~90% of components | 2 |
| `$app/state.page` / `$app/navigation.goto` | ~16 call sites | `useParams()` / `useNavigate()` | `useRouter()` / `useSearchParams()` | 1 |
| `@storybook/sveltekit` + `@storybook/addon-svelte-csf` | 185 `.stories.svelte` files, 721 play fns | `@storybook/react-vite` + CSF3 `.stories.tsx` | `@storybook/nextjs` + CSF3 | 3 |
| `@testing-library/svelte` component tests | 2 files | `@testing-library/react` | Same | 1 |
| `eslint-plugin-svelte` + `prettier-plugin-svelte` | 1 ESLint config | `eslint-plugin-react` + `eslint-plugin-react-hooks` | Same | 1 |
| `@sveltejs/adapter-static` | 1 config | Vite `build` (no adapter needed) | `output: 'export'` — breaks dynamic routes | 1 |
| `svelte.config.js` | 1 file | Deleted; config moves into `vite.config.ts` | `next.config.js` | 1 |
| `svelte-check` type-checking | CI step | `tsc --noEmit` + ESLint | Same | 1 |
| `__storybook-stubs__/*.svelte.ts` stubs | 7 files | Rebuilt as plain `.ts` stubs for Zustand stores | Same | 1 |
| `SvelteMap` (reactive Map, 4 files) | 4 files | Immer-backed Map in Zustand, or Map in Valtio proxy | Same | 2 |

**Framework-agnostic assets that port unchanged:** All `$lib/api/**` TypeScript modules, all Tailwind v4 CSS and design tokens, `idb`, `tailwind-variants`, `tailwind-merge`, `clsx`, `markdown-it`, `dompurify`, `culori`, `apca-w3`, `apcach`, `@internationalized/date`, all 721 Storybook play function bodies (`@storybook/test` is framework-agnostic), 27 of 29 unit test files (pure TS), all 5 integration tests, Vite dev-server proxy config.

### Coupling Scorecard

| Dimension | Coupling | Port difficulty 1–5 |
|---|---|---|
| State / Reactivity | HIGH | 4 — auto-tracked deps become manual dep arrays; rune singletons need Zustand design review |
| Components / UI Primitives | HIGH (isolated behind `ui/` wrapper) | 3 — bits-ui replaced by Radix/shadcn; wrapper layer absorbs the blast radius |
| Routing / Data Loading | MEDIUM | 2 — no SSR, 3 load functions, 16 navigation call sites |
| Storybook | HIGH | 3 — 185 files rewritten, but play bodies unchanged |
| Testing | LOW | 1 — 27/29 unit tests agnostic; 2 files need `@testing-library/react` swap |
| Build / Tooling | MEDIUM | 2 — Vite/Tailwind/PostCSS stay; swap SvelteKit plugin, svelte-check, ESLint plugins |

### Effort Range and Phase Breakdown

| Phase | Scope | Effort (person-weeks) | Notes |
|---|---|---|---|
| P0: Calibration spike | One complex route end-to-end (`/sessions/[sessionId]`) | 1 PW | Required before committing; revises all estimates |
| P1: Scaffold + tooling | React + Vite dual config, TanStack Router, TS, ESLint, CI | 2–3 PW | One-time cost |
| P2: Design system + primitives | 35–40 headless components (bits-ui → shadcn/Radix), icon swap | 4–6 PW | Spike Combobox/DatePicker in week 1 |
| P3: State layer | 25 `.svelte.ts` singletons → Zustand stores, WebSocket hook, IDB bindings | 5–8 PW | Highest design risk; every module needs review not just translation |
| P4: Route groups (overlaps P3) | 14 route groups, ~181 components, 4–8 components/day with LLM assist | 12–22 PW | Wide range; reactive data routes at the low end |
| P5: Stories (alongside P4) | 185 `.stories.svelte` → `.stories.tsx` + Storybook re-validation | 8–14 PW | No automation; 5–15 min/simple story, 30–90 min/complex |
| P6: Tests | 34 test files (only 2 need framework swap; setup/teardown patterns differ) | 3–5 PW | |
| P7: Cutover | Remove Svelte plugin/SvelteKit, bundle audit, performance validation | 2–3 PW | |
| Buffer (unknowns) | Reactivity regressions in QA, IDB edge cases | +20–30% | Standard migration contingency |

**Total (1 senior engineer):** 36–61 person-weeks (~9–15 months).
**Total (2 engineers, stories and routes concurrent):** 22–35 calendar weeks (~6–9 months).
**Total (3-person team, P2/P3/P4 overlapping):** 16–24 calendar weeks (~4–6 months).

### Migration Strategy: Incremental Strangler-Fig (Only Viable Option)

Big-bang rewrite fails at this scale. The validated pattern: React shell + TanStack Router as host router replaces SvelteKit router; Svelte routes are lazy-loaded per route segment; the dual-framework Vite config (`plugins: [react(), svelte()]`) is confirmed to work in production. Svelte plugin and all Svelte deps are removed last, after the final route group cutover. The dual-runtime overhead during transition (~140–200KB) is acceptable for a developer-facing local tool.

---

## 2. What We Gain — and What We Lose

### What We Gain

**Ecosystem breadth and long-term maintenance.** React has the largest hiring pool, broadest library ecosystem, and longest commercial support history of any UI framework. For a codebase maintained over 5+ years with rotating contributors, onboarding a React developer costs less than onboarding a Svelte 5 developer.

**Stable TanStack Table + Virtual path.** As of April 2025, TanStack Table's Svelte 5 adapter required a community workaround via huntabyte's `shadcn-svelte` `createSvelteTable`, and TanStack Virtual had known element-binding bugs requiring manual `$virtualizer._willUpdate()` calls. The React path is the fully-tested, documented, stable route. This is the single most concrete library gain for a data-heavy inspection UI with virtualized request lists.

**shadcn/ui registry breadth.** 50+ components vs shadcn-svelte's smaller set; same architectural model but larger community troubleshooting surface.

**cmdk command palette (React-only).** The most polished command palette in any framework. For an operator tool with 14 route groups and complex filtering, this would be high-value UX.

**Recharts + shadcn/ui chart blocks (React-first).** No maintained Svelte port. If charts are added to the diagnostics or session pages, Recharts integrates directly with the shadcn/ui design system.

**React Compiler 1.0 (stable, Dec 2025).** Build-time automatic memoization via opt-in Vite plugin. Eliminates the manual `useMemo`/`useCallback` tax in new code, closing the ergonomic gap with Svelte's compiler-driven reactivity.

**Tooling ecosystem.** React DevTools, TanStack Query DevTools, TanStack Router DevTools, Stack Overflow coverage, LLM training data, and AI-assisted coding tools all have deeper React-specific coverage.

### What We Lose / Risks

**Bundle size: 3× larger.** React 19 real-app bundle is ~156KB gzipped vs Svelte 5's ~47KB. Irrelevant for a localhost internal tool; a real regression on paper.

**Fine-grained reactivity.** Svelte compiles to direct DOM mutations with no VDOM diffing. React re-runs the component function on every state change. With Zustand selectors + TanStack Virtual + React Compiler, the practical runtime gap is small for this architecture — but `SessionTimeline`'s 14+ deep `$derived` chains and the ring-buffer WebSocket pattern in `ws.svelte.ts` are genuinely harder to translate correctly.

**Less boilerplate.** Svelte 5 runes are concise and auto-track dependencies. The equivalent React code with explicit `useMemo`/`useEffect` dependency arrays is more verbose and carries a stale-closure footgun class that Svelte eliminates by design.

**Auto-tracked deps → manual dep arrays.** The most common React migration bug class. A `$derived` that auto-tracked reads becomes a `useMemo` where every dependency must be enumerated explicitly. Silent stale-state bugs result from misses. The 25 `.svelte.ts` singletons with deep `$derived` chains (especially `shellState.svelte.ts`, `ws.svelte.ts`, `sessionDetailStore.svelte.ts`) are the highest-risk items — each requires a design review, not just a translation. This is the single highest-risk, most expensive phase of the migration.

**SvelteKit routing parity.** SvelteKit already provides typed path params via generated `$types`. TanStack Router's genuine gain over SvelteKit is typed search params and devtools — not typed path params. This is a capability regression risk if routing choice is not made carefully.

**No migration automation.** No production-ready Svelte→React codemods exist. LLM-assisted conversion breaks on reactivity semantics, bits-ui slot composition, and `update()` patterns. Every reactive state module requires human design review.

---

## 3. What Libraries It Unlocks

The "React-only?" column is the critical honesty check. Most widely claimed "React gains" are cross-framework.

| Need | Current Svelte approach | React library | React-only? | Real gain |
|---|---|---|---|---|
| Server-state / caching / invalidation | Hand-rolled idb + REST (no auto-invalidation) | TanStack Query v5 (`@tanstack/react-query`) | **No** — `@tanstack/svelte-query` v6 is runes-native on query-core v5 | LOW — adoptable today in Svelte |
| Large tables, sort/filter | Custom components | TanStack Table v8 (`@tanstack/react-table`) | **No** — Svelte adapter exists; had Svelte 5 workarounds as of Apr 2025 | **MEDIUM** — React path is fully stable and documented |
| Row virtualization (large request lists) | Manual / partial | TanStack Virtual v3 (`@tanstack/react-virtual`) | **No** — Svelte adapter exists; had element-binding bugs in Svelte 5 | **MEDIUM** — React path is stable |
| Grid with rich features | None | AG Grid Community | **No** — framework-agnostic, Svelte adapter exists | LOW |
| MUI X Data Grid Pro (virtualization gated) | None | MUI X Pro ($180/dev/year) | **Yes** — React-only | MEDIUM (paywall; TanStack covers free alternative) |
| Headless UI primitives | bits-ui + shadcn-svelte | shadcn/ui (Radix / Base UI backing) | **No** — shadcn-svelte mirrors the architecture | LOW — component count gap only |
| Deep WCAG accessibility | bits-ui (partial) | React Aria Components (Adobe) | **Yes** — no Svelte port | LOW for this app (operator-facing, no WCAG mandate) |
| Charts / sparklines | None (D3 option) | Recharts (shadcn/ui default chart layer) | **React-first** — no maintained Svelte port | **MEDIUM** if charts are planned |
| Custom dense timeline visualization | D3 directly | visx (Airbnb) | **Yes** — no Svelte port | LOW (D3 in Svelte is equivalent) |
| Form state with schema validation | Svelte `$state` + `bind:` | React Hook Form + Zod | **No** — Zod is agnostic; Svelte superforms is equivalent | LOW for this app's form complexity |
| Type-safe routing | SvelteKit generated `$types` | TanStack Router | **No** — SvelteKit already covers typed path params | NONE / regression risk |
| Command palette | None | cmdk + shadcn/ui Command | **Yes** — cmdk is React-only; Svelte alternative is less battle-tested | **MEDIUM** if planned (high-value UX for 14 route groups) |
| Animation | Svelte native `transition:`/`animate:` | Framer Motion | **No** — Svelte built-in covers inspection UI needs | LOW |
| Drag and drop | svelte-dnd-action (mature) | dnd-kit | **No** — Svelte has a mature equivalent | NONE |

**Genuine React-only or React-smoother unlocks for this app** (all others in the table are cross-framework):
- TanStack Table + Virtual on Svelte 5: rough path smoothed to stable (most concrete gain)
- cmdk command palette (React-only, high-value if planned)
- Recharts + shadcn/ui chart blocks (React-first; needed if charts are added)
- shadcn/ui registry breadth (50+ vs fewer in shadcn-svelte)

---

## 4. React SPA vs Next.js

Both options evaluated against the actual architecture: pure client SPA on `:9990`, Rust axum backend on `:9989` over REST + WebSocket, no public users, no SEO, dynamic IDs derived from a runtime Postgres DB.

### Next.js Static Export (Option A)

`output: 'export'` emits static HTML/CSS/JS — same end result as `vite build`. What is lost per official docs: Server Actions, Route Handlers, ISR, Middleware, image optimization, and — critically — **dynamic routes without `generateStaticParams()`**. The 4 dynamic route shapes (`[sessionId]`, `[agentId]`, `[id]`, `[promptSha256]`) require runtime DB reads to enumerate IDs; they cannot be statically pre-rendered. Dynamic routing breaks under static export for this app without client-side workarounds that defeat the routing system.

**Verdict: blocked.** Static bundle with more configuration and a broken dynamic-route story. React + Vite produces the same output with less friction.

### Next.js with Node Server (Option B)

Run `next start` as a 4th process. Enables RSC, Server Actions, Middleware. Honest accounting:

- RSC fetch to `:9989` is a localhost HTTP hop — no data-proximity win vs a direct client fetch.
- WebSocket data is live: SSR renders a stale snapshot; the client re-subscribes immediately. The SSR step is pure overhead.
- ~90% of components land on `'use client'` — session timeline, live feeds, request body viewer, all interactive state. RSC covers cold initial page renders only.
- ISR is irrelevant: data is real-time captured traffic, not cacheable.
- Middleware is unused: no auth wall, no geo-distribution for a local dev tool.
- A CVSS 10.0 RCE via RSC deserialization ("React2Shell") and two follow-on CVEs within one week in late 2025/early 2026 require rapid upgrade cycles for any RSC-adopting team.
- Server/client boundary audit on every component adds cognitive overhead during an already-complex migration.

**Verdict: not justified.** Adds a Node process, a duplicate serialization hop on every data fetch, ongoing server/client boundary cognitive tax across 181+ migrated components, and exposure to a CVE cluster — for RSC, SSR, and ISR that are all irrelevant to this architecture.

### Clear Pick: React 19 + Vite

Matches the current architecture exactly. No server runtime tax. Simpler deployment. Migration effort goes to replacing Svelte components and rune state rather than to framework-specific configuration.

**Recommended stack:** React 19 + Vite + `@vitejs/plugin-react-swc` + TanStack Router v1 + TanStack Query v5 + Zustand v5 + shadcn/ui (Tailwind v4) + TanStack Virtual v3 + lucide-react.

The only scenario that changes this: a public-facing, SEO-indexed surface added to the same repo. That is a separate product decision requiring a separate evaluation.

---

## 5. Recommendation

**Verdict: Conditional No-Go.** Not a technical veto — React is sound and the migration is feasible. It is a timing and priority veto:

- **Gains are modest for this tool.** The library-unlock analysis (§3) shows most claimed React wins available in Svelte today; genuine React-only gains are smoothed TanStack Table/Virtual friction, cmdk, and Recharts — none currently in the app.
- **Effort is large relative to impact.** 36–61 person-weeks solo (§1 phase table) for an operator-facing localhost tool whose value is in its Rust backend.
- **The state layer is the migration's highest risk.** The 25 `.svelte.ts` rune singletons each require a design review, not a mechanical translation (§2).
- **The honest case for migration is hiring and long-term maintenance, not feature capability.** If the operator expects rotating contributors over 5+ years with Svelte 5 as a documented hiring constraint, migration earns its cost. If the team is stable and Svelte-proficient, it does not.
- **Next.js is rejected; React + Vite is the only viable React target.** See §4 for the full argument — both static export and the Node-server option are settled non-starters.

### Conditions Under Which Migration Becomes Go

Migration is justified if **any two** of the following conditions are true:

1. The operator plans to expand the team and Svelte 5 hiring is a documented constraint.
2. Charts, a command palette, or rich data-grid features are on the 6-month roadmap (the most concrete React-only capability gains for this app).
3. The migration is resourced at 2–3 engineers, keeping calendar time under 6 months.
4. A calibration spike (`/sessions/[sessionId]` end-to-end in React, 1 week) produces an effort estimate below 40 person-weeks total, validating the lower range of the current estimate.
5. The operator explicitly accepts the migration cost as a long-term investment in ecosystem longevity, and has a written record of that decision.

**If migration is conditionally approved:** Mandate the 1-week calibration spike before committing. Port `/sessions/[sessionId]` end-to-end — WebSocket hook, one bits-ui primitive in Radix, IDB binding — before any further commitment. The spike output replaces the research estimate's 70% variance with measured data.

**If migration is deferred:** The highest-value no-migration improvements that address the same motivations: adopt `@tanstack/svelte-query` v6 (eliminates the hand-rolled idb/REST cache, available today), apply the documented TanStack Virtual Svelte 5 workaround (or wait for the fix), and evaluate `svelte-command-palette` for the command palette UX improvement.

---

## Source Index

- **File 01** (`01-codebase-coupling.md`): structural coupling inventory, coupling scorecard, rewrite surface count (~330 files), framework-agnostic asset list
- **File 02** (`02-react-spa-target.md`): React 19 stack selection, Zustand vs Jotai, TanStack ecosystem, React Compiler 1.0 (stable Dec 2025), bundle size benchmarks (~3× larger), WebSocket + IDB patterns (TkDodo pattern, `experimental_createPersister`)
- **File 03** (`03-nextjs-target.md`): Next.js static export dynamic-route limitation, RSC/SSR relevance analysis, App Router CVE cluster (CVSS 10.0, late 2025–early 2026), server/client boundary cost, unanimous verdict: React + Vite
- **File 04** (`04-react-ecosystem-libs.md`): library-by-library gain analysis with "available in both" column; TanStack Query Svelte v6 (runes-native confirmation), TanStack Virtual Svelte 5 friction (element-binding bugs documented), cmdk React-only verdict, Recharts React-first verdict, honest net-gains summary
- **File 05** (`05-migration-mechanics.md`): big-bang vs strangler-fig analysis, phase-by-phase effort breakdown, LLM throughput estimates (4–10 components/day with LLM assist), reactivity translation risk table, 8-item risk register, calibration spike recommendation

**Reconciliation note:** Files 02 and 04 both address TanStack Query — file 02 recommends it as the React target for server state, file 04 confirms the Svelte v6 adapter exists and is runes-native. Both are correct and non-contradictory: TanStack Query is the right React choice AND it could be adopted without migrating. No substantive conflicts between files found on the Next.js verdict; files 02, 03, and 05 are unanimous.

---

## Verification note (orchestrator spot-check, 2026-06-21)

The acceptance gate independently verified the report's load-bearing codebase facts against the live repo. Findings:

| Claim in report | Verified against repo | Status |
|---|---|---|
| `shellState.svelte.ts`, `ws.svelte.ts`, `sessionDetailStore.svelte.ts` (named high-risk rune modules) | All three exist under `src/lib/stores/**` and `src/lib/` | ✅ real |
| `+page.ts` / `+page.server.ts` load functions = 3 | Exactly 3 | ✅ exact |
| `@lucide/svelte` import sites = 76 | 75 files | ✅ ~exact |
| `bits-ui` footprint | 86 files reference it (broader than the wrapper layer) | ✅ confirms HIGH coupling |
| `ui/*.svelte` primitive wrappers = 68 | **34** wrapper files (`*/ui/*.svelte`, excluding stories) | ⚠️ inflated ~2× |
| Storybook `play` functions = 721 | **585** `play:` keys | ⚠️ inflated (~+23%) |
| `SvelteMap` usage = 4 | 2 files | ⚠️ minor (usage sites vs files) |

**Impact on conclusions: none.** Both inflated counts overstate the rewrite surface in the same direction; the corrected figures (34 primitive wrappers, ~585 play functions) still support the same effort range and the same Conditional No-Go verdict. The qualitative claims — named rune singletons, HIGH state coupling, no migration automation, dynamic-route block on Next.js static export — are sound.

**One external claim left unverified:** the "React2Shell" RSC RCE / Next.js CVE cluster (§4, sourced from file 03) could not be confirmed from this environment. It is **not load-bearing** — Next.js is rejected on architecture (dynamic routes + a redundant serialization hop) regardless — so treat the CVE detail as flagged-pending-verification, not as a settled fact, before citing it elsewhere.
