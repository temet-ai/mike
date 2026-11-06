---
title: Usability & UI Polish Reading List
status: active
created: 2026-06-28
updated: 2026-06-28
summary: Curated, deduplicated reference links on plain-language copy, onboarding/tours, contextual help, accessibility, visual coherence, and design systems — the evidence base behind the usability-engineer agent and its skills.
---

# Usability & UI Polish — Reading List

The evidence base behind the `usability-engineer` agent (`.claude/agents/usability-engineer.md`)
and its skills (`plain-language-microcopy`, `contextual-help-disclosure`, `invokable-page-tour`,
`ui-coherence-state-audit`). Grouped by topic; every link is a durable, high-signal source.

## Plain language & UX writing

- [NN/g — Plain Language Is for Everyone, Even Experts](https://www.nngroup.com/articles/plain-language-experts/) — evidence that even domain experts prefer plain, jargon-free copy. Kills the "our users are technical so jargon is fine" excuse that justifies system-noun labels.
- [NN/g — Concise, SCANNABLE, and Objective: How to Write for the Web](https://www.nngroup.com/articles/concise-scannable-and-objective-how-to-write-for-the-web/) — the measured 124% usability gain from concision + scannability. The empirical case for halving the words and front-loading the outcome.
- [NN/g — The 3 I's of Microcopy (Inform, Influence, Interact)](https://www.nngroup.com/articles/3-is-of-microcopy/) — a framework for classifying every piece of microcopy by its job.
- [GOV.UK — Content design: writing for GOV.UK](https://www.gov.uk/guidance/content-design/writing-for-gov-uk) — the canonical plain-English method: start from what the user wants to know; short common words over formal ones.
- [GOV.UK — A to Z style guide](https://www.gov.uk/guidance/style-guide/a-to-z) — concrete word-by-word rulings you can adopt directly as house style.
- [plainlanguage.gov — Federal Plain Language Guidelines](https://www.plainlanguage.gov/guidelines/) — write for your audience, cut jargon and hidden verbs, be concise, with explicit "words to avoid" lists.
- [Mailchimp Content Style Guide — Voice and Tone](https://styleguide.mailchimp.com/voice-and-tone/) — separating a constant voice from a context-dependent tone (calm in errors, light only in genuine success).
- [Shopify Polaris — Error messages](https://polaris-react.shopify.com/content/error-messages) — say what happened, explain the fix, offer a one-click action; max two sentences.
- [Apple HIG — Writing](https://developer.apple.com/design/human-interface-guidelines/writing) — verb-led button labels ("Send Payment" over "Submit"), plain, inclusive wording.

## Onboarding & in-product tours

- [NN/g — Onboarding Tutorials vs. Contextual Help](https://www.nngroup.com/articles/onboarding-tutorials/) — the push-vs-pull distinction and the "easy to dismiss AND recall" rule. The canonical case for contextual help over a forced upfront tour.
- [NN/g — Onboarding: Skip It When Possible (video)](https://www.nngroup.com/videos/onboarding-skip-it-when-possible/) — why the best onboarding is often minimal or absent; design the product to be self-evident first.
- [Appcues — SaaS User Onboarding Examples (20+ products)](https://www.appcues.com/blog/saas-user-onboarding) — concrete activation patterns (Figma template-first canvas, Slack conversational setup, deferred team setup). What "done well" looks like.
- [Appcues GoodUX — Notion's Lightweight Onboarding](https://goodux.appcues.com/blog/notions-lightweight-onboarding) — avoiding the blank-canvas problem with templates and inline walkthrough tooltips.
- [Growth.Design — UX Case Studies (47+ teardowns)](https://growth.design/case-studies) — psychology-grounded teardowns of real flows; the model for diagnosing why a step helps or hurts.
- [DesignerUp — I studied 200+ onboarding flows](https://designerup.co/blog/i-studied-the-ux-ui-of-over-200-onboarding-flows-heres-everything-i-learned/) — a meta-analysis distilling cross-cutting patterns and anti-patterns.

## Contextual help & progressive disclosure

- [NN/g — Tooltip Guidelines](https://www.nngroup.com/articles/tooltip-guidelines/) — tooltips are supplementary only, must trigger on hover AND focus, stay brief, never hide vital task info.
- [NN/g — Why So Many Info Tips Are Bad (and How to Make Them Better)](https://www.nngroup.com/articles/info-tips-bad/) — the three info-tip failure modes (redundancy, hiding critical info, interrupting flow) with fixes.
- [NN/g — Progressive Disclosure](https://www.nngroup.com/articles/progressive-disclosure/) — show what most users need first, defer the rest; keep the primary set small.
- [NN/g — Timing Guidelines for Exposing Hidden Content](https://www.nngroup.com/articles/timing-exposing-content/) — hover/reveal delay reasoning; informs tooltip open/close delays.
- [GOV.UK Design System — Details component](https://design-system.service.gov.uk/components/details/) — a production disclosure for help relevant only to a subset of users.
- [NHS digital service manual — Hint text](https://service-manual.nhs.uk/design-system/components/hint-text) — plain-language hint-text rules: a few words, never essential info.

## Accessibility & ARIA patterns

- [WAI-ARIA APG — Dialog (Modal) Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/) — the exact roles, initial focus, focus-trap, Escape, and focus-return rules a tour step (a modal dialog) must implement.
- [WAI-ARIA APG — Tooltip Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tooltip/) — focus + hover trigger, `aria-describedby` wiring, Escape to dismiss, with working code.
- [WAI-ARIA APG — Disclosure (Show/Hide) Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/) — a real button with `aria-expanded` + `aria-controls`; the accessible basis for "learn more" detail.
- [WAI-ARIA APG — patterns index](https://www.w3.org/WAI/ARIA/apg/patterns/) — open the specific pattern before hand-rolling any interactive widget.
- [web.dev — Keyboard focus](https://web.dev/learn/accessibility/focus) — focus order, focus management, and `:focus-visible` in practice, with code.

## Visual coherence, states & loading

- [Refactoring UI (Wathan & Schoger)](https://www.refactoringui.com/) — design on constrained scales, start in grayscale, build hierarchy with size/weight/contrast before color, generous whitespace.
- [Refactoring UI — "Top 20 Key Points" summary](https://medium.com/design-bootcamp/top-20-key-points-from-refactoring-ui-by-adam-wathan-steve-schoger-d81042ac9802) — a free, skimmable distillation of the book's rules.
- [Material Design 3 — Interaction states](https://m3.material.io/foundations/interaction/states/applying-states) — the canonical state model with state-layer opacity values; a concrete spec for how each state should look.
- [NN/g — Button States Communicate Interaction](https://www.nngroup.com/articles/button-states-communicate-interaction/) — why each button state must be visually distinct; the "why" behind the state matrix.
- [NN/g — Skeleton Screens vs. Progress Bars vs. Spinners](https://www.nngroup.com/videos/skeleton-screens-vs-progress-bars-vs-spinners/) — when to use each loading pattern and why to prefer skeletons sized to the final layout.
- [NN/g — Designing Empty States in Complex Applications](https://www.nngroup.com/articles/empty-state-interface-design/) — the three empty-state types and the rule that an empty state must instruct and offer a path forward.
- [web.dev — Optimize Cumulative Layout Shift](https://web.dev/articles/optimize-cls) — how layout shift is measured (CLS ≤0.1) and its causes; the measurement behind the no-reflow rule.
- [Carbon Design System — Empty states pattern](https://carbondesignsystem.com/patterns/empty-states-pattern/) — a production empty-state anatomy (message + primary action).

## Design systems & pattern libraries

- [Shopify Polaris](https://polaris.shopify.com/) — color roles, spacing tokens, per-component documented states in production.
- [IBM Carbon Design System](https://carbondesignsystem.com/) — working code paired with human-interface guidelines.
- [Atlassian Design System](https://atlassian.design/) — strong content guidelines and patterns (messaging, empty states, onboarding).
- [GOV.UK Design System](https://design-system.service.gov.uk/) — the gold standard for plain-language, accessibility-first patterns with research-backed do/don't.
- [GitHub Primer](https://primer.style/) — open-source system with clear component docs and UI-text guidelines.

## Live UI reference galleries

- [Mobbin](https://mobbin.com/) — the largest curated library of real app screens and flows; search by pattern.
- [Page Flows](https://pageflows.com/) — recorded end-to-end flows from real products.
- [UI-Patterns.com](https://ui-patterns.com/patterns) — a taxonomy of named interaction patterns and the problem each solves.

## Tour libraries (evaluate before adopting)

- [driver.js](https://driverjs.com/) — lightweight, dependency-free, **MIT**-licensed spotlight/tour library. The default if a tour library is warranted.
- [Shepherd.js](https://shepherdjs.dev/) — flexible guided tours with strong keyboard/a11y focus and Floating UI positioning. Note **AGPL/commercial** licensing.
- [intro.js](https://introjs.com/) — study as a cautionary a11y case (links-as-buttons, missing ARIA, no focus trap) and an **AGPL/commercial** example. What to avoid.
- [React tour library benchmark (2026)](https://usertourkit.com/blog/react-tour-library-benchmark-2026) — head-to-head on downloads, bundle size, licensing, and axe-core violations.
- [Floating UI](https://floating-ui.com/) — the positioning engine for anchoring coach-mark/spotlight popovers so they stay attached on scroll/resize.
