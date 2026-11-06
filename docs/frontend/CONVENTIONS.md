---
title: Frontend Component Conventions
status: active
created: 2026-07-29
updated: 2026-07-29
summary: Code-level conventions for Svelte components and stories. The visual system lives in Storybook's Foundations stories — this file never restates it.
supersedes: docs/frontend/style/component-patterns.md, docs/frontend/style/storybook-conventions.md, docs/frontend/style/tokens.md, docs/frontend/style/typography.md, docs/frontend/style/components.md, docs/frontend/style/patterns.md, docs/frontend/style/common-mistakes.md, docs/frontend/style/adoption-gaps.md, docs/frontend/style/DIGEST.md, docs/frontend/THEMEABLE_DESIGN_SYSTEM.md
---

# Frontend Component Conventions

**The visual system is Storybook's Foundations stories** (`frontend/src/lib/components/__foundations__/`,
[open them](http://localhost:6006/?path=/story/foundations-style-guide--core)). Colors, type,
spacing, motion, the edge contract and every component's rendered appearance are defined there, live, with a
theme toggle. They are the only source for visual decisions; [`STYLE_GUIDE.html`](STYLE_GUIDE.html) is the
design export FUNK started from, reference only. GitHub shows it as source; its sections are drawn as
pictures in [`style-guide/`](style-guide/), with the text in [`style-guide/guide.txt`](style-guide/guide.txt).

**This file is code conventions only** — how components are structured, typed and storied. It deliberately
carries no color values, no token names and no shape rules. If you are about to write one here, it belongs in
the gallery instead.

## Two tiers, no third

| Tier | Path | Contains |
|---|---|---|
| **Primitive** | `src/lib/components/ui/<name>/` | Generic, headless, reusable. Button, Dialog, Tabs, Tooltip. |
| **Domain** | `src/lib/components/<area>/` | Project composites that import primitives. SessionRow, TurnCard, FilterWorkbench. |

**Primitives** import only `bits-ui`, `$lib/utils`, `svelte/elements`, `tailwind-variants`, `@lucide/svelte`.
No stores, no `$lib/api`, no `$lib/types`, no hardcoded copy — text arrives via props.

**Domain components** compose primitives and never re-implement one. They may import stores, types and API
modules, and may hold project-specific copy.

Files are `PascalCase.svelte` in this repo. That is the convention — do not "migrate" to kebab-case.

## Component structure

Merge every `class` prop through `cn()` (`$lib/utils`) so callers can override utilities without specificity
fights. Interpolating `{className}` into a class string instead is a silent override failure.

Destructure `$props()` in this order — ref, class, variants, snippets, rest:

```svelte
<script lang="ts">
	let {
		ref = $bindable(null),
		class: className,
		variant = 'default',
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> & { variant?: BadgeVariant } = $props();
</script>

<div bind:this={ref} data-slot="badge" class={cn(badgeVariants({ variant }), className)} {...restProps}>
	{@render children?.()}
</div>
```

- **`restProps`** is the name, always. Grepping for it finds primitives.
- **`ref = $bindable(null)`** — omitting `$bindable()` silently prevents parent binding.
- **`data-slot="<component-name>"`** on every primitive root. It gives CSS and tests a hook that does not
  depend on utility classes, and enables parent/child selectors (`has-data-[slot=…]`) that class composition
  cannot express.
- **`{@render children?.()}`**, never `<slot />`. The `?.()` is mandatory — `children` is optional and calling
  an undefined snippet crashes.
- **2+ variants or sizes → `tailwind-variants`** in a `<script module>` block, exported alongside its
  `VariantProps` type. Not an inline `{#if}` picker, not a lookup object — those cannot express
  `defaultVariants` or compound variants and export no usable type.
- **`WithElementRef` / `WithoutChildren`** (`$lib/utils`) type any primitive wrapping a native element, so
  `{...restProps}` passes arbitrary HTML attributes through instead of growing a prop per attribute.

## Interaction

**Polymorphic `href`.** A component that can link renders `<a>` when `href` is set, otherwise
`<button>`/`<span>` — via `<svelte:element>` or `{#if href}`. Never ship a parallel `ButtonLink`; the two
drift. A disabled anchor needs `aria-disabled`, `tabindex={-1}`, `href={undefined}` and `role="link"` — an
`<a>` cannot otherwise be disabled.

**Clickable non-buttons** (rows, cards, tiles) need all six or `svelte-check` fails on
`a11y-click-events-have-key-events`: `role="button"`, `tabindex="0"`, an explicit `aria-label`,
`aria-expanded` when it toggles, an `onkeydown` covering **Enter and Space**, and a visible focus style. A
clickable child inside a clickable row calls `e.stopPropagation()`.

**Collapse/expand** uses `open = $bindable(true)` plus `aria-expanded` on the trigger. App-wide persistence
goes through the shared collapse store (`$lib/stores/collapseState.svelte.ts`), never local `$state` — see
[`convention_collapsestore_expand_persistence`]. Chevrons are Lucide components.

**Overlays and form controls wrap `bits-ui`** — one file per part, Root re-exporting the bits-ui prop type,
Content adding project classes and the portal. Do not hand-roll a dialog, popover, select or tooltip.

**Motion and depth come from the shared layer, never from the component.** A component says which motion it
takes with a `data-motion` attribute and which layer it sits on with `data-elevation`; the timing, the easing
and the shadow are decided once in `src/lib/components/ui/motion.css` and `elevation.css`. Do not write a
`transition`, an `animation` or a `@keyframes` in a component — a duration written locally is a duration
nothing can keep in step with the rest of the app. One motion, `wait`, is allowed to repeat, because a wait
has no settled state until it ends; it must move something directional (a sweep, a rotation, a march) and must
never oscillate opacity or scale, which is the pulse this codebase does not ship. Mount and unmount are the one thing CSS cannot see, and
their transitions live in `ui/motion.ts` so they still read the same tokens. The Storybook galleries
**Foundations/Motion** and **Foundations/Elevation** are the reference: they render every motion and every
layer role live on the real primitives, say which state change each one answers to, and record why a surface
clipped by augmented-ui has to hang its shadow on an unclipped ancestor. Read them before adding motion or
depth to anything.

**Icons are `@lucide/svelte`, never Unicode glyphs.** A glyph is announced as "right-pointing small triangle",
ignores `size-*`, and renders stroke-thin. Import named icons individually — the package is tree-shakable,
bulk imports are not. An em-dash used as copy is copy, not an icon; leave it.

## Stories

Every reusable component has a co-located `<PascalName>.stories.svelte`. Storybook is the source of truth for
component behavior and runs the a11y gate.

**One component → one story file → one fully-functional representative `<Story>`.** Per-theme, per-state,
per-size and design-exploration variants are banned; see
`.claude/rules/frontend-development.md`
for the full rule and its `__foundations__/*` exemption.

- Fixtures come from factory functions, not inline literals shared between stories.
- Components reading stores or `$app/*` need a `__harness__/` wrapper that injects them; stories import the
  harness, not the component.
- Route stories inject their data via context (e.g. `detailById`) rather than fetching.
- Interactive components carry a `play` test.
- **No raw hex.** Stories are checked for it like production code — the gallery's tokens are the only source.
- **Preserve inspection content.** Lean views need a full-content route; use scrolling or virtualization for complete items. Binding:
  `.claude/rules/agent-behavior-and-data.md`.

Run them with `scripts/sb-test` — the single frontend command (typecheck + unit + Storybook browser tests).
Not `pnpm test`, not `vitest --project storybook` directly. `--json` gives a structured verdict.

## Binding rules

These rules govern the implementation:

- Agent behavior and data: preserve captured content; lean inspection views need a full-content route; traffic mutation and content egress are opt-in.
- Frontend development: reuse components, follow the test tiers and keep one representative story per component.
- Visual design and UI: use the Foundations design system; passive updates preserve position and state changes repaint without reflow.
