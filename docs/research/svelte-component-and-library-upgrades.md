---
title: "Svelte Component & Library Upgrades — Better Components Without a Port"
status: active
created: 2026-06-21
updated: 2026-06-21
summary: Survey of Svelte-adoptable library upgrades and better component options for the temet-proxy operator inspection UI, without requiring a React port.
session_id:
  - b00952ce-0019-46d8-b9cf-a7a27dd8e0b6
---

# Svelte Component & Library Upgrades — Better Components Without a Port

This document is the **no-port path**: library and component improvements adoptable in the
current SvelteKit 2 + Svelte 5 runes stack. It is a sibling to
`docs/research/svelte-to-react-migration.md` (if a port is ever reconsidered, see that
document). Every recommendation here is evaluated against the app's real needs: a
data-heavy operator inspection UI with large request/session tables, virtualized long lists,
a session timeline, live WebSocket updates, filter and compose forms, and IndexedDB caching.

---

## 1. Baseline — What the App Uses Today

From the coupling inventory (`artifacts/2026-06-21/svelte-to-react/01-codebase-coupling.md`):

| Layer | Current choice | Assessment |
|---|---|---|
| Headless UI primitives | `bits-ui` (Dialog, Tooltip, Popover, Tabs, Separator), wrapped in 68 `ui/*.svelte` files | Good. The wrapper layer insulates app code from bits-ui directly. |
| Icons | `@lucide/svelte` | Good. Tree-shaken, comprehensive, maintained. |
| Styling | Tailwind v4 + `tailwind-variants` + `tailwind-merge` + `clsx` | Good. No change needed. |
| Server/API state | Hand-rolled: manual `fetch` + custom `BoundedLruCache` IDB layer | Fragile. No automatic invalidation, no background refetch, no optimistic updates. Biggest gap. |
| Tables | Custom hand-rolled components | Functional but limited. No sort/filter/column-resize primitives. |
| Virtualization | Ad-hoc or absent | Missing. Critical for the request list at scale. |
| Markdown rendering | `markdown-it` + `dompurify` | Fine for this use case. No change needed. |
| Dates | `@internationalized/date` | Fine. |
| Forms | Native `$state` + `bind:` | Adequate for the current form complexity. Not a bottleneck. |
| Command palette | None | High-value gap for 14+ route groups. |
| Charts | None | Optional; timeline uses DOM layout, not SVG charts. |

---

## 2. Server State & Caching — `@tanstack/svelte-query` v6

### What it replaces

The hand-rolled `BoundedLruCache` + IDB layer (`cacheIdb.ts`, `apiCache.svelte.ts`,
`requestDetailStore.svelte.ts`, etc.) manages: in-memory LRU caching, IndexedDB
persistence, in-flight dedup, and manual invalidation via `refresh()` calls. It works,
but every store must re-implement stale detection, background refresh, and error recovery
by hand.

### What TanStack Query v6 delivers in Svelte

`@tanstack/svelte-query` v6 shipped as a full runes-native rewrite. It runs on
`@tanstack/query-core` v5 (the same engine as the React adapter) and requires Svelte
≥ 5.25.0. The API surface:

```ts
// Rune-based thunk pattern (required for reactivity)
const query = createQuery(() => ({
  queryKey: ['requests', sessionId],
  queryFn: () => fetchRequests(sessionId),
  staleTime: 30_000,
}))
// Access directly — no $ prefix needed
$: query.data, query.isPending, query.isError
```

Key capabilities over the hand-rolled cache:

- **Automatic background refetch** on window focus + stale interval — the current cache
  never refetches unless the user navigates or calls `refresh()` manually.
- **Request deduplication** across concurrent components — the hand-rolled cache does
  this via an in-flight Map, but it must be maintained per store.
- **Optimistic updates** via `useMutation` — relevant for filter rule edits and compose
  operations that currently POST and then manually refresh the list.
- **Devtools** (`@tanstack/svelte-query-devtools`) for debugging cache state in the
  inspection UI itself.
- **WebSocket sync**: `queryClient.invalidateQueries(['sessions'])` from any WS event
  handler is cleaner than the current cursor-polling pattern scattered across `$effect`
  calls. The WS ring-buffer event still drives the invalidation; TQ handles the refetch
  and cache update.

### IDB persistence via the persister

The existing IDB data can be persisted through TanStack Query's persister layer:

- `@tanstack/query-persist-client-core` + `idb-keyval` = per-client full persistence
  (all cache restored on reload).
- `experimental_createPersister` with an IDB-backed AsyncStorage adapter = lazy
  per-query persistence (loads only when a `createQuery` call needs it — more memory
  efficient for a large cache).

The current hand-rolled IDB layer can be **replaced entirely** by the persister. The
`idb` package already in use can be swapped for `idb-keyval` (lighter, same API shape).

### Testing-isolation caveat (operator concern, 2026-06-21)

**The reason to adopt this is testability — not "a newer cache."** The current hand-rolled IDB cache
is the *source* of the Storybook/frontend testing pain: stories must inject `detailById` to avoid a
load-error banner, `SessionTimeline` froze on a batch-warm `$effect` loop, and IDB reads had to be
timeout-guarded so a wedged store degrades to network. **Swapping one cache tool for another that
reproduces that pain is not worth doing** (operator, emphatic 2026-06-21).

TanStack Query is worth adopting here *only* if it removes that pain — which it can, but only when
configured for isolation:

- **The IDB persister is opt-in.** In unit / Storybook / integration runs, construct a fresh
  `QueryClient` with **no persister**, `gcTime: Infinity`, and seeded `queryClient.setQueryData(...)`.
  Stories then render from injected data with **zero IndexedDB** — eliminating the exact
  `detailById`-injection and wedged-read failure modes rather than relocating them.
- **One persister config, not per-store IDB handles.** The wedged-read / partial-warm failures came
  from many bespoke IDB code paths. A single persister config is one well-tested path with documented
  per-test utilities (`QueryClientProvider` + a per-test client).

**Decision gate:** adopt only if a 1-store spike proves the Storybook/test path runs persister-free and
the prior failure modes do not recur. If the spike reproduces the isolation pain, **do not proceed** —
the swap has bought nothing.

### Effort and payoff

**Effort: M.** Migrating one store at a time is viable. The `$lib/api/*.ts` fetch
functions stay unchanged; they become `queryFn` arguments. The `setContext`-provided
stores (`sessions.svelte.ts`, `filterRules.svelte.ts`, etc.) can be migrated
incrementally — TQ can coexist with the hand-rolled stores during a transition.

**Payoff: HIGH.** This is the single highest-ROI adoption in this stack. It deletes
~500 lines of cache-management code and gives the app background refresh, dedup, and
optimistic updates for free. The WS integration becomes cleaner. The IDB layer
consolidates into a single persister config instead of per-store IDB handles.

**Sources:**
- [TanStack Svelte Query overview](https://tanstack.com/query/latest/docs/framework/svelte/overview)
- [Svelte 5 support discussion (shipped)](https://github.com/TanStack/query/discussions/7413)
- [IDB persister discussion](https://github.com/TanStack/query/discussions/3198)
- [Per-query persister pattern](https://lucas-barake.github.io/persisting-tantsack-query-data-locally/)

---

## 3. Tables & Virtualization

### 3a. Tables — `@tanstack/svelte-table` (honest status check)

**Current Svelte 5 state (as of June 2026):**

- `@tanstack/svelte-table` v8 (latest stable: 8.21.3) does **not** support Svelte 5. It
  uses `svelte/internal` and breaks at runtime. The npm package for v8 targets Svelte 3/4.
- `@tanstack/svelte-table` v9 is in alpha and is rune-native, but it changes frequently
  and breaks frequently. Not production-ready.

**Viable workaround today:** `shadcn-svelte` ships a `data-table` component that wraps
TanStack Table v8 using `createSvelteTable` from a community shim. It works reliably
in Svelte 5 as long as you go through shadcn-svelte's component source rather than the
npm adapter directly. Multiple community reference implementations exist
([walker-tx](https://github.com/walker-tx/svelte5-tanstack-table-reference),
[stefa168](https://github.com/stefa168/svelte5-tanstack-table-examples)).

**Verdict:** For the request/session tables, adopting TanStack Table core via the
shadcn-svelte data-table component is viable today. It adds column-level sort, filter,
and pagination primitives that the current hand-rolled tables lack. Accept the workaround
and plan to migrate to the official v9 adapter once it stabilises.

### 3b. Virtualization — `virtua` vs `@tanstack/svelte-virtual`

Two options:

**`virtua`** (~3kB, zero-config, Svelte 5 native via `virtua/svelte`):
- Component-based: wraps an items array with a `VList` or `VGrid` component.
- Handles dynamic item sizing, reverse scrolling, and iOS scroll behaviour out of the box.
- Zero configuration to get a working virtual list. Best for the virtualized request list
  where items are uniform-height bands.
- [GitHub](https://github.com/inokawa/virtua) | [npm](https://www.npmjs.com/package/virtua)

**`@tanstack/svelte-virtual`**:
- Headless primitives: you own the markup and positioning.
- Pairs naturally with TanStack Table for virtualised table rows.
- As of April 2025 had Svelte 5 element-binding bugs requiring `$virtualizer._willUpdate()`
  manually. Current status: the Svelte adapter ships with documented table examples, but
  verify the binding issue is resolved before adopting.
- Best when tight integration with TanStack Table row models is needed.
- [TanStack Virtual Svelte docs](https://tanstack.com/virtual/v3/docs/framework/svelte/svelte-virtual)

**Recommendation:** Use `virtua` for the request list (zero-config, Svelte 5 native).
Use `@tanstack/svelte-virtual` for the session/request data-table if going the TanStack
Table route — their row-model integration is more natural despite the workaround cost.

**Effort: S–M.** `virtua` is a drop-in wrapper around the existing list render. TanStack
Virtual alongside TanStack Table is M because it requires rethinking the table markup.

**Sources:**
- [TanStack Table Svelte v9 alpha docs](https://tanstack.com/table/alpha/docs/framework/svelte/svelte-table)
- [Svelte 5 + TanStack Table workaround (James O'Claire)](https://jamesoclaire.com/2025/04/09/easiest-way-to-get-tanstack-table-v8-working-with-svelte-5/)
- [TanStack Virtual Svelte 5 issue](https://github.com/TanStack/virtual/issues/866)
- [Virtua GitHub](https://github.com/inokawa/virtua)

---

## 4. Better Component Kits — the Current Svelte UI-Kit Landscape

The app already wraps bits-ui behind a `ui/` layer (shadcn-svelte model: app code imports
from `$lib/components/ui/dialog`, not from bits-ui directly). This is the right
architecture. The question is whether to stay with bits-ui, augment with shadcn-svelte's
component model, or switch to a styled kit.

### Option A — Keep bits-ui, augment via shadcn-svelte (Recommended)

**bits-ui** is the headless primitive layer. **shadcn-svelte** sits on top of bits-ui,
adds Tailwind styling, and exposes a CLI to copy component source into the project (same
model as shadcn/ui React: you own the code, no version lock-in, fix bugs yourself).

The current `ui/` wrapper layer already follows this model. The recommended upgrade is:

1. Install the shadcn-svelte CLI and pull components you do not currently have.
2. Let the CLI regenerate your existing primitives (Dialog, Tooltip, Popover, Tabs) with
   the current shadcn-svelte styling and Svelte 5 snippet patterns.
3. Add missing high-value components: `Command` (command palette), `DataTable` (TanStack
   Table wrapper), `Chart` (LayerChart wrapper), `Combobox`, `DatePicker`.

**Coverage as of 2026:** shadcn-svelte has 40+ components, expanding continuously.
A daily-updated comparison is at
[shadcn-compare](https://github.com/jasongitmail/shadcn-compare) — check there before
writing a component from scratch; it may already exist.

**Tailwind v4 fit:** shadcn-svelte 2.x supports Tailwind v4. The CSS-first token model
maps cleanly onto the app's existing `data-theme` OKLCH token system.

**Why not switch away from bits-ui?** bits-ui is the primitive layer shadcn-svelte is
built on. Switching would mean re-implementing the same accessibility behaviour from
scratch. The right answer is not to choose between them — they are complementary layers.

### Option B — Skeleton UI v3

Skeleton v3 launched with full Svelte 5 runes support (all components rewritten for runes,
snippets, and Tailwind v4). It also ships a React adapter. The trade-off:

- **More opinionated than bits-ui/shadcn-svelte.** Components come pre-styled with
  Skeleton's design language. Overriding that design to match the app's current OKLCH
  token system is non-trivial.
- **Built on Zag.js** primitives (cross-framework behaviour layer) rather than Melt UI.
  This is a different accessibility and interaction model from bits-ui.
- **Good for greenfield.** For an existing app with a tuned design system and a custom
  token layer, adopting Skeleton v3 means swapping one design language for another — a
  larger visual regression risk than augmenting via shadcn-svelte.

**Verdict:** Skip for this app. The custom OKLCH token system and existing styling is
already invested. Skeleton's value is a complete design system out of the box; here the
design system already exists.

### Option C — Flowbite Svelte

Flowbite Svelte is an official Svelte port of Flowbite: 60+ pre-styled components, dark
mode, Tailwind CSS, actively maintained. Like Skeleton, it imposes a design language.

**Verdict:** Skip for this app. Same reasoning as Skeleton: an opinionated visual system
over a codebase that already has one creates churn without improving capability. The
individual components from Flowbite are not better than shadcn-svelte's on accessibility
or interactivity depth.

### Recommendation

**Keep bits-ui as the headless primitive layer. Adopt the shadcn-svelte component model
to augment the existing `ui/` wrapper layer.** This is the lowest-churn highest-gain path:

- The `ui/` wrapper architecture is already the shadcn-svelte model.
- Pull missing components (Command, DataTable, Chart) via the CLI.
- Regenerate existing primitives for Svelte 5 snippet patterns.
- Zero design system disruption.

**Sources:**
- [shadcn-svelte docs](https://www.shadcn-svelte.com/docs)
- [bits-ui docs](https://bits-ui.com/docs/introduction)
- [shadcn-compare daily matrix](https://github.com/jasongitmail/shadcn-compare)
- [Skeleton v3 launch](https://github.com/skeletonlabs/skeleton/discussions/3372)
- [Svelte UI library comparison 2026](https://svar.dev/blog/how-to-choose-svelte-library/)

---

## 5. Command Palette

14 route groups, nested settings, filter workbench, prompt composition, steering editors —
this is a navigation problem that a command palette would address well. No palette exists
today.

### Options

**Bits UI `Command` component (Recommended)**

`cmdk-sv` (the original cmdk port) is now **deprecated**. The maintainer (Huntabyte) has
officially superseded it with the **Command** component in Bits UI, which shadcn-svelte
exposes as its `<Command>` block. The shadcn-svelte Command supports:

- Full-screen dialog with ⌘K / Ctrl+K trigger.
- Groups, separators, icons, keyboard shortcut hints.
- Filtered search over a flat or grouped item list.
- Built-in ARIA roles and keyboard navigation.
- Zero additional dependency — Command is already in bits-ui.

If the app adopts shadcn-svelte (recommended above), the Command component is a one-CLI-command
addition: `pnpm exec shadcn-svelte@latest add command`.

**svelte-command-palette (rohitpotato)**

A standalone alternative: ~5kB, Fuse.js fuzzy search, `defineActions` API, Svelte 5-ready,
actively maintained. Best if you want a fully batteries-included palette with fuzzy matching
across keywords and descriptions, without tying to the bits-ui stack.
[GitHub](https://github.com/rohitpotato/svelte-command-palette) |
[Demo](https://svelte-command-palette.vercel.app/)

**Verdict:** Use Bits UI `Command` if adopting shadcn-svelte (no new dependency, already
in the stack). Use `svelte-command-palette` if you want standalone turnkey fuzzy search
without the shadcn-svelte adoption step.

**Effort: S.** Command palette is a new feature with no existing code to migrate. Wiring
14 route destinations into a `defineActions` or shadcn-svelte Command item list is a
half-day task.

**Sources:**
- [cmdk-sv deprecated, points to Bits UI Command](https://github.com/huntabyte/cmdk-sv)
- [shadcn-svelte Command docs](https://www.shadcn-svelte.com/docs/components/command)
- [svelte-command-palette](https://svelte-command-palette.vercel.app/)

---

## 6. Charts / Visualization

The app has no charting library today. The session timeline is a DOM-layout lane
structure (not SVG charts). Potential future needs: request volume over time (sparklines
on the sessions page), error-rate trends on the diagnostics page.

### LayerChart (shadcn-svelte's choice)

**LayerChart** is the official charting backend for shadcn-svelte's `<Chart>` component
blocks. Built on D3 primitives with Svelte-native composability. v2 (Svelte 5 migration)
is in pre-release but actively evolving. The shadcn-svelte Chart block wraps it and
provides sensible defaults.

- For simple sparklines and time-series bars (the most likely need here), the higher-level
  `BarChart` / `LineChart` components in LayerChart v2 handle the scaffolding.
- Tailwind is optional (actively being made so).
- Pre-release status means breaking changes are possible — pin the version.

[LayerChart changelog](https://www.layerchart.com/changelog) |
[shadcn-svelte Chart docs](https://www.shadcn-svelte.com/docs/components/chart)

### D3 directly

For the existing session timeline (lane-based, DOM-positioned), D3 for SVG drawing would
be a natural fit if the timeline ever needs richer visualisation. D3 is framework-agnostic
and works idiomatically in Svelte — bind scales and selections in `$derived` values, update
in `$effect`.

### Verdict

**Skip for now.** The app's current timeline uses DOM layout; no chart need is urgent.
When charts are needed (diagnostics page, volume sparklines), adopt shadcn-svelte's Chart
block (which pulls LayerChart). The shadcn-svelte CLI makes this a one-command add when the
need arises. Avoid a standalone charting dependency until then.

---

## 7. Forms — Superforms + Formsnap

### What the app has today

Forms in the inspection UI (`filterRules` create/edit, `compose` POST body, `settings` save)
are built with native `$state` + `bind:`. They are simple: few fields, no multi-step
validation, no async validation, no nested arrays. The current approach works.

### What Superforms + Formsnap would add

**sveltekit-superforms** (`sveltekit-superforms` on npm) — fully Svelte 5 compatible,
actively maintained (Zod 4 support added 2026, CVE-2026-22774 patched). It gives:
- Type-safe form state with schema validation (Zod, Valibot, or 10+ others).
- Constraint inference from the schema (required, min/max populated automatically).
- Progressive enhancement for SvelteKit form actions (not relevant here — this is
  a client-side-only SPA with no form actions).

**Formsnap v2** — Svelte 5-native (snippets replace slots), wraps Superforms with
accessible form field components. Used as the form layer in shadcn-svelte.

### Honest verdict for this app

The forms here are 3–8 fields each, all client-side, with no server round-trip validation.
Superforms's primary value proposition is server-side form handling in SvelteKit form
actions. Without that, it is a Zod-schema-to-form binding layer. That binding layer is
useful but not transformative for 5-field forms with `$state`.

**Verdict: Evaluate, don't rush.** If a shadcn-svelte adoption brings in the Form component
(which wraps Formsnap), use it for new forms and refactor existing ones opportunistically.
Do not adopt Superforms as a standalone upgrade for the current forms — the benefit does
not justify the migration cost at this form complexity level.

**Sources:**
- [sveltekit-superforms docs](https://superforms.rocks/)
- [Formsnap v2 migration guide](https://www.formsnap.dev/docs/v2-migration-guide)
- [shadcn-svelte Form (Formsnap + Superforms)](https://www.shadcn-svelte.com/docs/components/form)

---

## 8. Verdict Table

| Need | Current approach | Recommended Svelte adoption | Effort | What it improves | Verdict |
|---|---|---|---|---|---|
| Server state & caching | Hand-rolled IDB + `BoundedLruCache` | `@tanstack/svelte-query` v6 + idb-keyval persister | M | Auto-invalidation, background refetch, dedup, optimistic updates, ~500 LOC deleted | **Adopt now** |
| Request list virtualization | Absent or ad-hoc | `virtua` (`virtua/svelte`) | S | Smooth scrolling of 10k+ request items, zero config | **Adopt now** |
| Command palette | None | Bits UI `Command` (via shadcn-svelte) | S | Keyboard navigation of 14 route groups, discoverability | **Adopt now** |
| Component kit augmentation | bits-ui + hand-rolled `ui/` wrappers | shadcn-svelte CLI (regenerate + add missing) | S–M | Svelte 5 snippet patterns, more components, CLI-driven, no lock-in | **Adopt now** |
| Session/request data tables | Custom hand-rolled components | TanStack Table v8 via shadcn-svelte DataTable shim | M | Sort, filter, pagination primitives; migrate to v9 adapter when stable | **Adopt now** (with workaround) |
| Table virtualization | Absent | `@tanstack/svelte-virtual` (alongside TanStack Table) | M | Virtualized table rows for 10k+ request grids | **Evaluate** — verify Svelte 5 binding issue resolved first |
| Forms | `$state` + `bind:` | Formsnap v2 + Superforms (via shadcn-svelte Form) | M | Schema-validated fields, accessible labels/errors | **Evaluate** — adopt opportunistically with shadcn-svelte |
| Charts / visualization | None | shadcn-svelte Chart (LayerChart v2 backend) | S (when needed) | Sparklines, time-series for diagnostics; one CLI command away | **Skip for now**, add when a concrete chart need arises |
| Icons | `@lucide/svelte` | No change | — | Already optimal | **Keep** |
| Markdown | `markdown-it` + `dompurify` | No change | — | Already optimal for this use case | **Keep** |

---

## Suggested Adoption Order

### Step 1 — `@tanstack/svelte-query` v6 (highest ROI)

Replace the hand-rolled IDB cache with TanStack Query. The `$lib/api/*.ts` functions
become `queryFn` arguments; the `BoundedLruCache` + per-store IDB handles are replaced
by a single `QueryClient` with an idb-keyval persister. Migrate one store at a time
starting with `sessions.svelte.ts`. Wire WS events to `invalidateQueries` instead of
manual `refresh()` calls. Payoff: automatic background refresh, dedup, optimistic updates,
and ~500 LOC deleted.

### Step 2 — `virtua` for the request list + shadcn-svelte CLI for components

These two are independent S–M tasks that can run in parallel:
- Drop `virtua/svelte`'s `VList` around the request band list. Zero config.
- Run `pnpm exec shadcn-svelte@latest add command data-table` to pull the Command palette and
  DataTable components. Regenerate existing primitives (Dialog, Tooltip, etc.) with the
  CLI to get Svelte 5 snippet patterns.

### Step 3 — Command palette wiring

Wire the Bits UI `Command` component with the 14 route destinations, settings actions,
and compose shortcuts. High UX impact for internal operator tooling.

### What to skip or defer

- **Skeleton UI / Flowbite Svelte** — design-system switches on an already-styled app; no
  net capability gain.
- **Superforms standalone** — not worth the migration for 5-field client-only forms.
- **Charts** — add the shadcn-svelte Chart block when a concrete chart is being built, not
  before.
- **TanStack Table v9** — wait for stable release before migrating off the shadcn-svelte shim.
