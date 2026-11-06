---
title: App-Wide Usability Audit Backlog
status: active
created: 2026-06-28
updated: 2026-06-28
summary: Prioritized usability backlog from an app-wide audit of 20 SPA surfaces against the four usability skills (60 confirmed findings, 11 false positives dropped).
---

# App-Wide Usability Audit Backlog

Produced by the `app-wide-usability-audit` workflow: 20 surface units, each audited against the four usability skills then adversarially verified. **60 confirmed findings**, 11 false positives dropped. Action items feed the `usability-engineer` agent; the `ui-usability-review` CI gate + live verification keep fixes honest.

## State of the UI

A first-time operator landing in this SPA hits a wall of undefined domain vocabulary and raw machine output. Across nearly every surface — home, session detail, request detail, settings, steering, analysis, agents, prompts, diagnostics, filter workbench, and the help guides themselves — internal nouns (steerer, analyzer, weasel intercept, fleet, authority/mode, graders, filter rules) and raw enum/snake_case tokens (verdicts like gate_miss/no_drift, feature keys like recall_injector, statuses, block[N]) render verbatim with no plain-language anchor, despite a shipped HelpHint/Tooltip primitive and a populated glossary (helpContent.ts) that already define many of them. The second systemic failure is silent state handling: several pages render empty/error/loading states indistinguishably (settings-embedding and settings-workers capture save/sync errors but never display them, so a failed PATCH looks like success; the request-detail timeline shows "No requests" during initial load; settings-steering/weasel strand the user on inert pages when a feature is absent), and the help guides actively misdirect users to a "Filter Rules drawer" and a "+ New rule" button that no longer exist. Compounding this, internal artifacts leak into user-facing copy — repo file paths, DTO/type names, SQL predicates (is_error = true), schema jargon (agent_id becomes NULL), and "fixture data"/"DATA GAP" build terms. The net effect is that the product's core mechanisms are undecodable and several primary paths to first success (create a filter rule, create a prompt composition, save embedding config) are either broken, misdocumented, or give no feedback. The fixes are overwhelmingly cheap microcopy and help-wiring against infrastructure that already exists.

## Cross-cutting themes (highest leverage — one fix, many surfaces)

### [HIGH] Domain nouns render as bare jargon with no help affordance, despite a shipped HelpHint/Tooltip primitive and a populated glossary
*help · 13 occurrences*

Adopt a project rule: every first occurrence of a glossary-backed domain term gets a HelpHint with 'Learn more →'. Audit each route, attach existing helpContent.ts slugs (steerer, fleet, weasel-pattern, authority/mode, bigbro, harness/subagent, sub-judges, finding), and add the few missing slugs (analyzer, finding). The infrastructure already ships on sibling routes — this is wiring, not building.

### [HIGH] Raw enum / snake_case / machine tokens shown verbatim as primary user-visible labels
*microcopy · 12 occurrences*

Introduce render-time label maps for every enum and feature key (verdicts, authority/mode, statuses, outcomes, feature keys, block[N], category de-slugs). Display the human label as primary text; keep the raw token reachable via title or a secondary mono tag. Keep API paths keyed on the raw key so the swap is safe.

### [HIGH] Task-essential meaning hidden in hover-only native title= on non-focusable elements
*help · 6 occurrences*

Ban native title= as the sole carrier of essential meaning on non-interactive spans (it is invisible to keyboard/touch/SR). Promote each to a click HelpHint (aria-describedby/aria-expanded) or an always-visible inline gloss. Covers authority/mode defs, prompt_sha256-fallback consequence, preview-unavailable, and disabled affordances.

### [HIGH] Captured error/empty/loading states never surfaced — failures and initial loads look like success or genuine emptiness
*coherence-state · 8 occurrences*

Audit every page's state matrix (loading / empty / error / feature-absent). Wherever an error is written to state, render it where the user acted and wire it into Retry. Add a distinct initial-load flag instead of reusing the empty branch, and add explicit feature-absent {:else} branches. This is the second-highest-leverage class after help.

### [HIGH] Internal implementation artifacts leak into user-facing visible text and titles
*microcopy · 11 occurrences*

Establish a 'no internal nouns in user-facing strings' check: no repo file paths, DTO/type names, raw routes, SQL predicates, schema jargon (agent_id/NULL), build terms ('fixture data', 'DATA GAP'), or query-string syntax in visible text or title attributes. Rewrite to the observable consequence; keep raw technical detail only in an explicitly-faint secondary operator line.

### [MEDIUM] Missing focus-visible and active/selected paint on interactive elements
*coherence-state · 4 occurrences*

Apply the existing focus-visible:ring pattern to every interactive row/anchor, and a paint-only selected style to every toggle-style row. All paint-only (ring/outline/background/border-color) so none violates no-layout-shift.

### [LOW] Hardcoded rgba() shadows instead of theme-aware elevation tokens
*coherence-state · 3 occurrences*

Define shared --shadow-* / --elevation-* tokens once and replace the duplicated rgba(0,0,0,…) literals on cards and popovers so depth stays consistent across themes.

## Top priorities (ranked)

### 1. [HIGH] Wire shipped HelpHint/Tooltip to every first-occurrence domain noun, backed by the existing glossary
**Surfaces:** home, session-detail, request-detail, analysis, settings, settings-steering, settings-analysis, diagnostics-agent-dispatches, filter-workbench, steering

**Why:** The single largest comprehension blocker: core mechanism nouns (Steerers, Graders, Filter Rules, Analyzers, Fleet, Authority/Mode, weasel intercept, BIGBRO, HARNESS/SUBAGENT, sub-judges) render as bare jargon on ~10 surfaces while helpContent.ts already defines most slugs and HelpHint.svelte is used on sibling routes. The infra exists; it is simply not attached.

**Fix:** Attach a click-tier HelpHint (with 'Learn more →' to /help) at the first occurrence of each domain term on each surface, reusing existing helpContent.ts slugs (steerer, fleet, weasel-pattern, steering-authority, steering-mode, bigbro, harness-message, subagent, refocus-steerer, verifier-steerer, steering-enrolment). Add missing slugs (analyzer, finding). Reserve the hint-icon footprint to satisfy no-layout-shift.

### 2. [HIGH] Decode raw enum / snake_case / machine tokens into human labels at render
**Surfaces:** home, session-detail, request-detail, analysis, settings, settings-weasel, prompts

**Why:** Verdicts (fired/no_drift/gate_miss/suppressed_*), authority/mode enums, feature keys (recall_injector), unmapped status strings, outcome chips, block[N] tokens, and analyzer=value chips are the primary user-visible content of feed lines, toggle rows, and status pills — not chrome. A first-timer reads them as opaque identifiers across many surfaces.

**Fix:** Map each enum/key to a plain label at render (e.g. fired→'Intervened', no_drift→'On track', recall_injector→'Recall relevant past context'), keep the raw token reachable via title/secondary tag. Decouple human label from the API path (which stays keyed on feature.key). No content is hidden, so inspection-fidelity is unaffected.

### 3. [HIGH] Fix the two help guides that direct users to UI that does not exist (phantom Filter Rules drawer / + New rule)
**Surfaces:** help-content

**Why:** Both the filter-rules guide and the anomalies guide instruct the first-timer to open a 'Filter Rules drawer' via the top-bar ≡ icon (which is actually the help drawer) and click '+ New rule' — neither exists; the drawer is explicitly 'retired' in source. This strands the user on a primary first-success path (create a filter rule) and contradicts the glossary's own correct seenAt.

**Fix:** Rewrite both guides to name the real surface: the filter workbench at /filter/workbench reached via the 'Filters' toolbar item, with the per-block AddRulePopover add-rule flow and the prefilled-form navigation. Fix both in one pass so they match the verified UI and glossary.

### 4. [HIGH] Display the save/sync errors that are captured in state but never rendered
**Surfaces:** settings-embedding, settings-workers

**Why:** On settings-embedding, model-select and enable-toggle failures write settingRowError keys that no element renders (the bool path even reverts the optimistic value), so a failed PATCH silently looks like it succeeded. On settings-workers, syncStatusError is set on catch but the error region never references it and Retry never re-runs loadSyncStatus, so a failed status query is indistinguishable from a genuinely down remote DB. Silent failure on a destructive-feeling action is among the worst trust breakers.

**Fix:** Surface every captured error where the user acted: pass an error prop into WorkerCard (toggle + model list) and render it inline, add syncStatusError to the error-region condition and the Retry handler. Feedback-only; touches no inspection content.

### 5. [HIGH] Move task-essential meaning out of hover-only native title= attributes (keyboard/touch/SR reachable)
**Surfaces:** session-detail, request-detail, prompts, filter-workbench, diagnostics-agent-dispatches

**Why:** Authority/mode definitions, fired/held semantics, the prompt_sha256-fallback consequence ('proxy will not match traffic'), and the preview-unavailable explanation live only in <abbr>/native title= on non-focusable spans — invisible to keyboard, touch, and screen-reader users, which violates contextual-help-disclosure for essential meaning.

**Fix:** Promote each to a click-openable, keyboard- and SR-reachable HelpHint (aria-describedby/aria-expanded) or an always-visible inline gloss; drop the native title= on non-interactive spans.

### 6. [HIGH] Rewrite empty/error states that lead with internal routes, build-state, and query-string syntax
**Surfaces:** prompts, analysis, diagnostics-memory, diagnostics-tool-errors

**Why:** Primary empty/error copy tells users to hand-assemble '?from=request:{id}', names raw routes (/api/prompts/compositions, /api/prompts/anomalies) and 'hasn't shipped yet' build-state as the lede, and surfaces 'inspect API' / 'is_error = true' / 'analyzer registry' as the first instruction — undecodable to a non-builder on the exact screens meant to guide first use.

**Fix:** Lead with the user's goal and a plain action ('Start from a captured request', 'Make sure the proxy is running, then retry'); turn prose pointers into real links/buttons; demote routes, ports, and raw errors to a secondary faint operator-detail line (which several of these already have).

### 7. [MEDIUM] Add real loading and feature-absent states so a successful-but-empty load is not an inert/false page
**Surfaces:** request-detail, settings-steering, settings-weasel

**Why:** The session timeline renders 'No requests in this session yet.' during initial load — false for any deep-linked non-empty session. settings-steering drops the whole Steerer card and renders inert OFF sub-judge cards when the feature list is empty; settings-weasel strands the user on a bare header. These are silent dead-ends with no explanation.

**Fix:** Add an initial-loadAll in-flight flag to sessionDetailStore and branch to a skeleton/'Loading requests…' on first paint; add explicit {:else}/empty-state branches ('No steerer configuration found — is the proxy running with the steerer subsystem enabled?', 'Weasel intercept is not available on this proxy build.') for the feature-absent case.

### 8. [HIGH] Stop leaking internal artifacts (repo paths, DTO/type names, schema jargon, 'fixture data'/'DATA GAP') into user-facing strings
**Surfaces:** session-detail, agents, filter-workbench

**Why:** Visible text and title attributes expose 'fixture data', 'DATA GAP', DTO/type names (SteererDto, SessionEnrolmentRow), repo file paths (dependency-findings.md §D2, filter-workbench-design.md), and DB schema jargon (agent_id becomes NULL) — meaningless or alarming to a non-builder and a leak of implementation internals.

**Fix:** Suppress sections until real data exists or label in user terms ('Sample data — live stats coming soon'); rewrite the delete-confirm to the observable consequence ('they just stop being grouped under this name'); never surface type names, repo paths, or build terms in visible text or title attributes.

### 9. [MEDIUM] Add visible focus-visible and active/selected states to interactive rows that only paint on hover
**Surfaces:** agents, analysis

**Why:** Agent/composition navigation rows give mouse users a hover border but leave keyboard users on the browser default outline (the pattern exists on the sibling empty-state link). Analysis 'By analyzer' filter rows reflect selection only in aria-pressed with zero visual change, so a sighted operator cannot see which filter is active.

**Fix:** Add focus-visible:ring-2 ring-[var(--ring)] to the row anchors and a paint-only selected background/border keyed on the active analyzer (both paint-only, no layout shift), plus a one-line caption that the analyzer rows filter the findings below.

### 10. [HIGH] Fix the undefined --border-color token making real chrome borders render in text color
**Surfaces:** agents

**Why:** --border-color is never defined as a CSS custom property anywhere in src/, so bare border-[var(--border-color)] on the agents page header rule, refresh/edit/delete buttons, and every edit-form input falls back to currentColor (text color) — visibly wrong borders on chrome the operator sees, while the rest of the app uses defined tokens.

**Fix:** Replace bare var(--border-color) uses with the defined border-[var(--border-default)] token, matching the surrounding app.

### 11. [MEDIUM] Replace blunt/raw error throws and contradictory tooltips with accurate, blameless copy
**Surfaces:** steering, diagnostics-memory

**Why:** The steering Step-2 banner pipes a bare `${res.status} ${res.statusText}` (e.g. '404 Not Found') straight to the user while every sibling path uses friendly copy; the memory page's 'RSS (peak)' tooltip says the value is RAM used 'right now', directly contradicting its own '(peak)' label and the documented getrusage peak semantics.

**Fix:** Throw/render a blameless actionable message keeping the raw status as secondary detail (mirroring the existing LoadErrorBanner); rewrite the RSS tooltip to describe the peak high-water mark so help matches the data.

### 12. [LOW] Route hardcoded rgba() shadows through shared elevation tokens for theme coherence
**Surfaces:** sessions-list, filter-workbench

**Why:** Card and popover drop-shadows use raw rgba(0,0,0,…) literals (duplicated across two session-list files) while the inset highlight beside them is already tokenized and the design system ships per-theme --shadow-* tokens — the literals read inconsistently across light/dark themes.

**Fix:** Define/use a shared elevation token (--shadow-card/--elevation-1, --shadow-lg/--shadow-md) once and apply it at all three sites, matching the already-tokenized var(--surface-shine).

## Per-surface breakdown

| Surface | High | Med | Low |
|---|---|---|---|
| home | 1 | 2 | 0 |
| sessions-list | 0 | 0 | 2 |
| session-detail | 3 | 2 | 1 |
| request-detail | 1 | 2 | 1 |
| analysis | 0 | 2 | 3 |
| prompts | 3 | 2 | 0 |
| agents | 1 | 3 | 1 |
| diagnostics-agent-dispatches | 0 | 1 | 2 |
| diagnostics-memory | 0 | 2 | 0 |
| diagnostics-tool-errors | 0 | 0 | 1 |
| filter-workbench | 2 | 0 | 2 |
| steering | 0 | 1 | 3 |
| settings | 1 | 3 | 1 |
| settings-analysis | 0 | 1 | 0 |
| settings-embedding | 1 | 0 | 0 |
| settings-steering | 0 | 2 | 2 |
| settings-weasel | 0 | 1 | 1 |
| settings-workers | 0 | 1 | 0 |
| help-content | 2 | 0 | 0 |

### Findings by surface

#### home
- **[high] Steering feed prints raw internal verdict tokens with no decoding** — `src/lib/components/home/SteeringZone.svelte:106,118`  
  Map each verdict to a human label at render (fired→'Intervened', no_drift→'On track', gate_miss→'Below threshold', suppressed_*→'Held back', failed_closed→'Failed safe'); keep raw token in title.
- **[medium] Fleet cells print raw authority/mode enum values as tags** — `src/lib/components/home/FleetZone.svelte:113-114`  
  Map authority/mode enums to plain words or attach a first-occurrence HelpHint defining advisory vs autonomous before the operator acts on the fleet.
- **[medium] Settings preview rows show raw snake_case config keys with no human label** — `src/lib/components/home/SettingsZone.svelte:101,125`  
  Render a human label per feature as primary row text (and in aria-label); keep the raw key secondary.

#### sessions-list
- **[low] Card shadows hardcode raw rgba(0,0,0,…) instead of an elevation/role token** — `frontend/src/routes/sessions/+page.svelte:905; frontend/src/lib/components/sessions/SearchBodyMatchRow.svelte:75`  
  Define a shared --shadow-card/--elevation-1 token once and use it in both .sl-row and .sbm-row, matching the already-tokenized var(--surface-shine).
- **[low] Body-match row exposes a raw internal request id with a terse 'req' prefix and no explanation** — `frontend/src/lib/components/sessions/SearchBodyMatchRow.svelte:49`  
  Drop the standalone 'req N' in favor of the labelled 'Request' nav link that follows it, or add a title explaining it identifies the captured request.

#### session-detail
- **[high] Core mechanism nouns (Steerers, Graders, Filter Rules, Analyzers) have no plain-language anchor and no Learn-more path** — `frontend/src/lib/components/shell/SessionMechanismsInventory.svelte:235,401,468,532`  
  Give each section a one-line always-visible plain sub-label or a click HelpHint backed by shared help content, plus 'Learn more →' to /help.
- **[high] Authority/Mode definitions and fired/held semantics live only in hover title= attributes** — `frontend/src/lib/components/shell/SessionMechanismsInventory.svelte:71-80,288-297,302-308`  
  Promote the definitions and a fired/held gloss to a click-openable, keyboard- and SR-reachable HelpHint or an inline gloss, instead of an <abbr> title only a mouse hover reveals.
- **[high] Internal 'fixture data' / 'DATA GAP' / DTO field-name developer jargon shown to end users** — `frontend/src/lib/components/shell/SessionMechanismsInventory.svelte:407-409,474-476,538-540,276,334,341`  
  Suppress these sections until real data exists or label as 'Sample data — live stats coming soon'; never surface 'fixture data', 'DATA GAP', or DTO/type names in visible text or titles.
- **[medium] Unknown backend status strings render verbatim into the Status pill** — `frontend/src/lib/components/shell/SessionOverview.svelte:160-166`  
  Map remaining known statuses to plain labels and fall back to a neutral 'Unknown'/'—' rather than echoing an internal status token.
- **[medium] Disabled 'turns ↗' affordance looks like a link, does nothing, and explains itself only in a hover title citing an internal artifact** — `frontend/src/lib/components/shell/SessionOverview.svelte:930-934`  
  Hide the affordance until it works or render a visible plain 'coming soon' state reachable without hover; do not surface internal artifact paths or present an inert link-styled control.
- **[low] Collapsed 'Session controls' summary stacks four undefined mechanism nouns into one tally line** — `frontend/src/lib/components/shell/SessionMechanismsInventory.svelte:198-207`  
  Acceptable once the mechanism nouns have a plain anchor (see help finding); until then it reads as jargon on the first visible content of the collapsed bar.

#### request-detail
- **[high] BigbroAttribution surfaces internal nouns and raw enum values with no plain-language or help** — `frontend/src/lib/components/detail/BigbroAttribution.svelte:226,239,251,270,278,288,302`  
  Attach HelpHint slug='bigbro' on the heading and slug='steering-authority'/'steering-mode'/'steered-turn' Tooltips on the authority/mode/verdict badges; rephrase 'Awaiting idle session return'; reserve hint footprint.
- **[medium] Collapsed band role markers HARNESS/SUBAGENT are undecodable internal nouns with no help** — `frontend/src/lib/components/shell/RequestBand.svelte:631,637,650,656`  
  Attach a first-occurrence HelpHint (slug harness-message/subagent) to the band-role marker or use a more self-describing label.
- **[medium] Timeline shows the empty state during initial load — no distinct loading state** — `frontend/src/lib/components/shell/SessionTimeline.svelte:1152`  
  Add an initial-loadAll in-flight flag to sessionDetailStore (existing loading/loadingOlder getters do NOT cover loadAll) and branch to a skeleton/'Loading requests…' on first paint, reserving the empty copy for confirmed-empty.
- **[low] Operator-turn outcome chip and STEERED chip render bare enum/jargon with no help** — `frontend/src/lib/components/shell/TurnBracket.svelte:221,241`  
  Render a human label for the outcome (or attach slug='outcome-corrected') and give the STEERED chip a slug='steered-turn' Tooltip.

#### analysis
- **[medium] 'analyzer' is a top-level grouping noun with no help affordance** — `frontend/src/routes/analysis/+page.svelte:540,555,591,845-847`  
  Attach a first-occurrence HelpHint to 'analyzer' at the 'By analyzer' heading and add an analyzer entry to helpContent.ts.
- **[medium] Analyzer filter chip exposes raw query-param key=value syntax** — `frontend/src/routes/analysis/+page.svelte:591`  
  Render a human chip like 'Analyzer: <name>' matching the sibling category/session chips instead of echoing 'analyzer='.
- **[low] Category filter chip shows regex-deslugged enum instead of the curated label** — `frontend/src/routes/analysis/+page.svelte:607`  
  Use CATEGORY_META[categoryParam].label in the chip instead of a regex de-slug of the raw enum.
- **[low] 'By analyzer' rows are filter toggles with no visible affordance or selected state** — `frontend/src/routes/analysis/+page.svelte:546-557`  
  Add a paint-only selected style keyed on the active analyzer plus a one-line caption indicating the rows filter the findings below.
- **[low] Error-banner hint leads with internal nouns instead of an action** — `frontend/src/routes/analysis/+page.svelte:459`  
  Lead with a plain action ('Make sure the proxy is running, then retry') and demote port and component names to secondary technical context.

#### prompts
- **[high] Primary empty-state heading instructs the user to hand-edit a URL query string** — `src/routes/prompts/new/+page.svelte:284-289`  
  Lead with the user's goal ('Start from a captured request') and turn prose references into real links/buttons rather than a query-string syntax the user assembles by hand.
- **[high] Compositions empty-state exposes build-backlog and a raw internal route as primary copy** — `src/routes/prompts/+page.svelte:356-363`  
  Rewrite the headline/body in blameless user language; keep the raw route and error in the existing faint operator detail line (365-367).
- **[high] Anomalies error empty-state surfaces a raw internal route name and build-state** — `src/routes/prompts/+page.svelte:409-416`  
  Rewrite headline/body in plain language ('Anomalies couldn't be loaded right now'); keep the route + raw error in the faint operator detail line (418-423).
- **[medium] Block cards lead with internal projection token block[N] instead of the plain descriptor** — `src/routes/prompts/[promptSha256]/+page.svelte:280-284`  
  Lead each label with the plain descriptor ('Billing header', 'Identity preamble', 'Instructions') and keep block[N] as a secondary mono tag.
- **[medium] prompt_sha256 fallback warning buries its task-essential consequence in a native hover-only title** — `src/routes/prompts/new/+page.svelte:350-358`  
  Surface the consequence ('proxy will not match traffic') as always-visible inline hint text near the save action or a click-accessible HelpHint, not solely a native title on a non-interactive span.

#### agents
- **[high] Undefined --border-color token: page/button/form borders render in text color, not a hairline** — `frontend/src/routes/agents/+page.svelte:75,84,156,170; frontend/src/routes/agents/[agentId]/+page.svelte:162,210,256,264,272,279,293,300,333`  
  Replace bare var(--border-color) uses with border-[var(--border-default)] for chrome/button/form borders.
- **[medium] Detail-page source chip is hardcoded amber for all three source values, contradicting the list** — `frontend/src/routes/agents/[agentId]/+page.svelte:194-199`  
  Apply the same per-source conditional classes (observed/file_backed/dynamic) on the detail chip so file-backed reads green and dynamic reads blue, matching the list.
- **[medium] Clickable agent/composition rows have no focus-visible style — keyboard focus is near-invisible** — `frontend/src/routes/agents/+page.svelte:170; frontend/src/routes/agents/[agentId]/+page.svelte:371`  
  Add focus-visible:ring-2 ring-[var(--ring)] to both row anchors, matching the empty-state link's existing pattern.
- **[medium] Delete-confirm dialog leaks the raw column name agent_id and SQL NULL to a non-builder** — `frontend/src/routes/agents/[agentId]/+page.svelte:315-318`  
  Rewrite to the observable consequence ('they just stop being grouped under this name; the captured prompt history is preserved'); drop agent_id and NULL.
- **[low] Terse 'req/24h' metric on composition rows is unexplained shorthand with no first-occurrence definition** — `frontend/src/routes/agents/[agentId]/+page.svelte:391-393`  
  Spell it out ('{n} requests in the last 24h') or add a Tooltip/HelpHint at this first occurrence.

#### diagnostics-agent-dispatches
- **[medium] 'Since (ISO 8601)' filter exposes a raw machine format with no help affordance** — `src/routes/diagnostics/agent-dispatches/+page.svelte:171-177`  
  Rename the label to plain language ('From date') and move the format requirement into a HelpHint/Tooltip, keeping the example placeholder.
- **[low] Project filter assumes the user knows the internal absolute project path** — `src/routes/diagnostics/agent-dispatches/+page.svelte:162-168`  
  Show a realistic example path in the placeholder or add a one-line HelpHint explaining the value is the absolute working directory the session ran in.
- **[low] '{brief_char_len} chars' meta is an unlabeled raw count** — `src/routes/diagnostics/agent-dispatches/+page.svelte:222`  
  Wrap the count in a Tooltip ('Full agent brief length: 1234 characters'), consistent with the adjacent timestamp and tool-id Tooltips.

#### diagnostics-memory
- **[medium] 'RSS (peak)' tooltip contradicts the label and the underlying data — says 'right now' for a peak value** — `frontend/src/routes/diagnostics/memory/+page.svelte:283`  
  Rewrite the tooltip to describe the peak high-water mark ('the highest physical RAM the proxy has used since it started') so help matches the getrusage peak semantics.
- **[medium] Blocking error names an internal system noun ('inspect API') a first-timer can't decode** — `frontend/src/routes/diagnostics/memory/+page.svelte:271`  
  Rewrite to a user-facing actionable phrase ('Can't load memory diagnostics — the proxy may be stopped. Check it's running, then retry.'); keep raw {memory.error} after it.

#### diagnostics-tool-errors
- **[low] Lede exposes the raw DB predicate is_error = true to a newcomer** — `src/routes/diagnostics/tool-errors/+page.svelte:225`  
  Drop the raw predicate from the PageIntro lede; say plainly 'Cross-session feed of every tool call that failed. Each row's content renders in full when expanded.' Keep the existing 'tool result' HelpHint.

#### filter-workbench
- **[resolved 2026-09-22]** The four findings below named `WorkbenchPreviewPane.svelte` and
  `SelectionActionPopover.svelte`, both removed in the filter-workbench redesign (spec
  `artifacts/2026-09-22/prompt-assistant/spec.md` §4): no End result pane, no floating selection
  popover. The 'backend dep'/'engine-truth (dry-run)' jargon and the file-path hover title do not
  exist in the new prompt pane (source / with-my-edits / final lenses); there is no longer a
  selection popover to carry a shadow token. Left here for the audit's own history rather than
  deleted, since the workflow that produced this backlog re-derives it from a live crawl.

#### steering
- **[medium] Session-overview load error surfaces a raw HTTP status line to the user** — `src/routes/steering/[id]/+page.svelte:273,285,645`  
  Replace the bare ${res.status} ${res.statusText} throw with a blameless actionable message keeping the raw status as secondary detail, mirroring the LoadErrorBanner (985) and model-assist fallback (741).
- **[low] Model-picker cost columns use unexplained '$/M in' / '$/M out' jargon** — `src/routes/steering/[id]/+page.svelte:1245,1252`  
  Add a Tooltip on each column header ('Input price per 1M tokens' / 'Output price per 1M tokens'), consistent with the sibling Model/Authority/Mode HelpCircle Tooltips.
- **[low] Suggestion-panel dismiss control uses a raw ✗ glyph instead of a lucide icon** — `src/routes/steering/[id]/+page.svelte:747-754`  
  Swap the literal ✗ for the lucide X icon at the same size to match other @lucide/svelte icons; aria-label is already correct.
- **[low] Model-picker row clips long model ids with ellipsis and no full-text affordance in the row** — `src/routes/steering/[id]/+page.svelte:1886-1890`  
  Add title={m.id} to the .mp-name span so the full id is reachable on hover (selection control, not an inspection content surface, so the escape hatch satisfies fidelity).

#### settings
- **[high] Feature toggle labels are raw machine keys (snake_case internal nouns)** — `frontend/src/routes/settings/+page.svelte:26-37`  
  Rename the human-facing label only ('Recall relevant past context', 'Project reminders'); the API path stays driven by feature.key (+page.svelte:170).
- **[medium] Page and section titles use Title Case and inventory jargon ('Catalogue')** — `frontend/src/routes/settings/+page.svelte:137,185-186`  
  Sentence case + user words: 'Settings' for the page title, 'All settings' / 'More settings' for the catalogue heading.
- **[medium] Feature descriptions lean on builder nouns 'session prompt' / 'inject' without defining them** — `frontend/src/routes/settings/+page.svelte:29-36`  
  Rewrite to the operator's mental model ('Adds this project's saved lessons and reminders to Claude's context at the start of each turn.'); drop wire-format verbs like 'inject'.
- **[medium] Domain terms in the subtitle (steerer, weasel intercept, embedding) are bold text with no help affordance** — `frontend/src/routes/settings/+page.svelte:138-143`  
  Wire each bolded term to a first-occurrence HelpHint backed by helpContent.ts (slug 'steerer' at 396, 'weasel-pattern' at 296) instead of bare <strong>.
- **[low] Switch accessible name announces the snake_case machine key plus the full description** — `frontend/src/lib/components/settings/FeatureToggleRow.svelte:92`  
  Build the accessible name from the human label and surface the description via aria-describedby (the error path already uses it at line 93).

#### settings-analysis
- **[medium] First-occurrence domain term 'Weasel-pattern' has no HelpHint, though glossary entry and HelpHint primitive exist** — `src/routes/settings/analysis/+page.svelte:140`  
  Attach <HelpHint slug="weasel-pattern" /> adjacent to the term's first appearance in the lede; the helpContent slug and HelpHint primitive both ship.

#### settings-embedding
- **[high] Model-selection and enable-toggle save errors are captured but never displayed — a failed save looks like success** — `src/routes/settings/embedding/+page.svelte:50-61,85-98`  
  Pass an error prop into WorkerCard for the toggle and model list and render it inline (or hoist a page-level alert) for settingRowError['embedding.model']/['embedding.enabled']; feedback only, no truncation.

#### settings-steering
- **[medium] Core domain terms ('Sub-judges', 'Refocus', 'Verifier', 'Correction classifier') have glossary entries but no HelpHint affordance on this page** — `frontend/src/routes/settings/steering/+page.svelte:185-188`  
  Wire the first occurrence of each domain noun to the shipped HelpHint backed by existing helpContent entries (steerer, refocus-steerer, verifier-steerer, fleet, corrected); inline descriptions stay as always-visible hints.
- **[medium] A successful-but-empty data load silently renders an inert page (Steerer card vanishes, sub-judge cards show empty OFF toggles)** — `frontend/src/routes/settings/steering/+page.svelte:173-188`  
  Add an explicit empty state for features.length===0 || steererFeature===null ('No steerer configuration found — is the proxy running with the steerer subsystem enabled?') rather than dropping the card and rendering inert sub-judge cards.
- **[low] Page lede uses 'the steerer fleet' / 'enrolled sessions' as proper nouns with no help link** — `frontend/src/routes/settings/steering/+page.svelte:142`  
  Attach a first-occurrence HelpHint to 'steerer fleet' (helpContent 'fleet') and 'enrolled' ('steering-enrolment').
- **[low] cfg() snippet paints an empty bordered card when no config keys resolve** — `frontend/src/routes/settings/steering/+page.svelte:192-201`  
  Suppress the container when zero rows resolve ({#if keys.some((k)=>byKey.has(k))}) or render an inline 'settings unavailable' note.

#### settings-weasel
- **[medium] Feature-absent case strands the user on a bare header with no message** — `src/routes/settings/weasel/+page.svelte:115`  
  Add an {:else} branch under {#if weaselFeature !== null} showing 'Weasel intercept is not available on this proxy build.' so a successful-but-empty feature list is distinguishable from a blank page.
- **[low] Field label says 'words' while its help text and the rest of the surface say 'phrases'** — `src/routes/settings/weasel/+page.svelte:125`  
  Rename the label to 'Forbidden phrases' to match its own help text, the lede, the card description, and what the control accepts.

#### settings-workers
- **[medium] Sync-status load failure is captured but never shown — a failed fetch renders as real worker data** — `src/routes/settings/workers/+page.svelte:116-122,87-103`  
  Add syncStatusError to the error-region condition and render it distinctly, add void loadSyncStatus() to Retry, or render an explicit error indicator inside the db_resync card so 'we couldn't check' is distinguishable from 'the remote DB is down'.

#### help-content
- **[high] Filter-rules guide instructs the user to open a phantom 'Filter Rules drawer' and click a non-existent '+ New rule'** — `src/routes/help/guides/filter-rules/+page.svelte:52-58,60-61`  
  Rewrite to the real surface: 'Open the filter workbench (the Filters button in the toolbar, or /filter/workbench)' and the per-block AddRulePopover add-rule flow, matching the verified glossary seenAt.
- **[high] Anomalies guide's quick-create step links to and describes the same phantom 'Filter Rules drawer'** — `src/routes/help/guides/anomalies/+page.svelte:76-81`  
  Change 'the Filter Rules drawer opens with the new-rule form prefilled' to the real surface — the filter workbench at /filter/workbench opening the prefilled new-rule form; fix in the same pass as the filter-rules guide.
