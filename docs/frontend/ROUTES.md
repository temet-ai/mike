---
title: Frontend Architecture — Routes, Components, Stores, API
status: active
created: 2026-06-21
updated: 2026-09-28
summary: Route-by-route audit of the SvelteKit SPA — route map, layout hierarchy, component inventory, stores, WebSocket integration, and the typed API client.
---

# Frontend Architecture — Routes, Components, Stores, API

Audit of `frontend/src/` as of 2026-07-07. The SPA is SvelteKit running under `adapter-static` in single-page mode (`+layout.ts` sets `ssr = false`, `prerender = false`), served by `vite dev` on `:9990`. The full app is one `index.html` fallback; SvelteKit handles routing client-side.

> No truncation rule applies. Component descriptions are short by design but **do not paraphrase** the captured content the components render — see `.claude/rules/agent-behavior-and-data.md`.

## 1. SvelteKit Route Map

Every file under `frontend/src/routes/`. Dynamic segments use `[name]` syntax; the URL column shows the runtime path.

| URL Path | Route file | Mounts / renders | Purpose |
| --- | --- | --- | --- |
| `/` | `routes/+page.svelte` | `SystemLivePanel`, `EmptyStateOnboarding` (when cold) | Home — live system overview; onboarding hint on zero traffic. |
| `/sessions` | `routes/sessions/+page.svelte` | Full paginated session list | All captured sessions, grouped by project. |
| `/sessions/[sessionId]` | `routes/sessions/[sessionId]/+page.svelte` | `SessionOverview` + `SessionTimeline` snippet; `SessionFlow` (lazy-loaded) behind a Requests/Flow tab strip, `?view=flow` | Session landing: overview dashboard with timeline revealed on scroll; Flow tab is the FUNK Flow Instrument (§3 below). |
| `/sessions/[sessionId]/requests/[requestId]` | `routes/sessions/[sessionId]/requests/[requestId]/+page.svelte` | `RequestDetailColumn` | Request drill-down with band timeline + heavy-bytes drawer. |
| `/agents` | `routes/agents/+page.svelte` | Inline list rows over `AgentListItem[]` (no extra component); `EmptyStateOnboarding` on cold start; `LoadErrorBanner` on failure | Catalogue of registered agents with version counts. |
| `/agents/[agentId]` | `routes/agents/[agentId]/+page.svelte` | Inline detail panels over `AgentDetail` + composition timeline; `LoadErrorBanner` on failure | One agent + every linked composition (newest-first). |
| `/analysis` | `routes/analysis/+page.svelte` | Inline severity-totals strip + category → session → request → finding tree; `EmptyStateOnboarding`, `HelpHint`, `LoadErrorBanner` | Findings dashboard: category → session → request → finding. |
| `/sessions/labelling` | `routes/sessions/labelling/+page.svelte` | `JudgeHome` with Turns / Sessions tabs (`?for=sessions`) | Labelling: how the judge labels your turns and sessions. Per tab: the on/off switch, the configuration in use (read-only, with Edit to change it and the command to label older items again after the change), the latest labelling accuracy, its alarms, labelling of older items in progress, its configurations and its settings. `?edit={configuration}` opens the change step with that configuration chosen. |
| `/sessions/labelling/accuracy` | `routes/sessions/labelling/accuracy/+page.svelte` | Labelling accuracy list | Every accuracy run per judge (tabs Turns / Sessions, `?for=sessions`), newest first by `created_at`, each a `TestRunCard` named by date, configuration and model: turns measured on, how often the judge gave the right label per question, turns left to review, and whether it passed the accuracy bar set for being put in use. |
| `/sessions/labelling/accuracy/[runId]` | `routes/sessions/labelling/accuracy/[runId]/+page.svelte` | Accuracy run page | What one accuracy run measured and how it scored, compared with the configuration in use, and "Use this run's configuration" (links to the Labelling page's change step). |
| `/sessions/labelling/accuracy/[runId]/review` | `routes/sessions/labelling/accuracy/[runId]/review/+page.svelte` | Review labels | One turn at a time where the judge's answer and the label differ, with the session around it (summary, the turn just before, earlier turns), the turn's evidence as attributed quotes, this turn's label and the judge's answer, and a pinned "Which is right?" panel where every answer is a button. Saves append gold labels tagged with the run while preserving its original labels, verdicts and metrics. |
| `/sessions/labelling/review` | `routes/sessions/labelling/review/+page.ts` | Load-only redirect | 307 to the newest complete turn-judge accuracy run's Review labels page (query kept), otherwise `/sessions/labelling/accuracy`. |
| `/diagnostics/tool-errors` | `routes/diagnostics/tool-errors/+page.svelte` | Tool-errors feed | Cross-request feed of `is_error: true` tool results. |
| `/diagnostics/agent-dispatches` | `routes/diagnostics/agent-dispatches/+page.svelte` | Agent-dispatch feed | Cross-session agent-dispatch drill-down. |
| `/memory` | `routes/memory/+page.svelte` | Memory page | Lessons with their outcomes and relations, the semantic worker's pattern candidates, and directive proposals awaiting a decision. |
| `/diagnostics/memory` | `routes/diagnostics/memory/+page.svelte` | Memory diagnostics dashboard | Process RSS, store budget, top-N entries, SPA eviction counters. |
| `/filter/workbench` | `routes/filter/workbench/+page.svelte` | `FilterWorkbench` | Filter-rule authoring: collapsible outline, prompt pane (As sent / With my edits / Final lenses), and a side panel splitting "Your edits" from the prompt assistant chat. |
| `/prompts` | `routes/prompts/+page.svelte` | `PromptKpiStrip`, `CompositionRow[]`, `AnomalyRow[]` | Prompt-composition management surface: KPI strip + classes table + anomalies table. |
| `/prompts/[promptSha256]` | `routes/prompts/[promptSha256]/+page.svelte` | `ClassChip`, `CompositionStatsHeader`, `BlockCard[]`, `PromptCompositionDiff` | Composition detail / drift view; mode toggle (observe/template/replace). |
| `/prompts/new` | `routes/prompts/new/+page.svelte` | `BlockCard[]` editor | Author a composition prefilled from `?from=request:{id}`. |
| `/prompts/volatile-lines` | `routes/prompts/volatile-lines/+page.svelte` | Volatile-line registry CRUD | Regex patterns stripped from `system[N]` before class hashing. |
| `/settings` | `routes/settings/+page.svelte` | Settings page | Hub: session-prompt features (reminders, recall_injector) + the general settings catalogue. Subsystems with their own page below are excluded here. |
| `/settings/assistants` | `routes/settings/assistants/+page.svelte` | Settings page | Prompt-assistant config: model, effort, instructions, what it can read/change, capture, budget, retention. |
| `/settings/embedding` | `routes/settings/embedding/+page.svelte` | Settings page | Embedding model + endpoint config for memory recall. |
| `/settings/flagging` | `routes/settings/flagging/+page.svelte` | Response flagging settings | Analysis master switch, five category toggles and false-positive filters, plus live correction (`weasel_intercept`) enablement, flagged phrases, correction prompt and buffer size. |
| `/settings/steering` | `routes/settings/steering/+page.svelte` | Settings page | Steerer fleet config (model, thresholds, sub-judges) + live steerer list from `/api/steerers`, the turn guard's questions panel and its calibration panel. |
| `/settings/history` | `routes/settings/history/+page.svelte` | Settings page | The configuration change timeline from `config.change_log` (`ChangeLogTable`), with a snapshot of the whole configuration at any past time. |
| `/settings/workers` | `routes/settings/workers/+page.svelte` | Settings page | Background worker fleet config (lesson worker, harvester, retention sweep, DB resync) + sync status and the alarms no other page claims; one line points to the Labelling page under Sessions. |
| `/steering` | `routes/steering/+page.svelte` | Steering hub | Live steering monitor; Fleet tab (`?tab=fleet`) shows steerer management. |
| `/steering/[id]` | `routes/steering/[id]/+page.svelte` | Steerer detail/edit | Per-steerer configuration and enrolment. |
| `/tools` | `routes/tools/+page.svelte` | Cross-session tool-call feed | Every tool use captured, filterable by name and project. |
| `/help` | `routes/help/+page.svelte` | Help landing | How-to guides + glossary + external references. |
| `/help/glossary` | `routes/help/glossary/+page.svelte` | Glossary grouped by domain | Vocabulary canonical source; HelpHint popovers deep-link here. |
| `/help/guides` | `routes/help/guides/+page.svelte` | Guide index | Lists every guide registered in `$lib/help/guides.ts`. |
| `/help/guides/anomalies` | `routes/help/guides/anomalies/+page.svelte` | `GuideLayout` | Prompt-anomalies how-to. |
| `/help/guides/filter-rules` | `routes/help/guides/filter-rules/+page.svelte` | `GuideLayout` | Filter rules how-to. |
| `/help/guides/getting-started` | `routes/help/guides/getting-started/+page.svelte` | `GuideLayout` | First-run walkthrough. |
| `/help/guides/inspecting-requests` | `routes/help/guides/inspecting-requests/+page.svelte` | `GuideLayout` | Request inspection how-to. |
| `/help/guides/retention` | `routes/help/guides/retention/+page.svelte` | `GuideLayout` | Retention / TTL guide. |
| `/help/guides/steering-concepts` | `routes/help/guides/steering-concepts/+page.svelte` | `GuideLayout` | Steering concepts how-to. |

**Redirect shims** (no `+page.svelte` — load-only):

- `routes/steerers/+page.ts` — 308 → `/steering?tab=fleet` (old `/steerers` bookmark compat)
- `routes/steerers/[id]/+page.ts` — 308 → `/steering/[id]` (old `/steerers/42` → `/steering/42`)
- `routes/settings/workers/evals/+page.ts` — 308 → `/sessions/labelling/accuracy`, query kept.
- `routes/settings/workers/evals/[runId]/+page.ts` — 308 → `/sessions/labelling/accuracy/[runId]/review`, query kept.
- `routes/judge-disagreements/+page.ts` — 308 → `/sessions/labelling/review`, query kept.
- `routes/judge-disagreements/runs/+page.ts` — 308 → `/sessions/labelling/accuracy`, query kept.
- `routes/judge-disagreements/[runId]/+page.ts` — 308 → `/sessions/labelling/accuracy/[runId]/review`, query kept.
- `routes/judges/+page.ts`, `routes/judges/turn/+page.ts` — 308 → `/sessions/labelling`, query kept.
- `routes/judges/session/+page.ts` — 308 → `/sessions/labelling?for=sessions`, other query kept.
- `routes/judges/runs/+page.ts` — 308 → `/sessions/labelling/accuracy` (`judge=session` becomes `for=sessions`).
- `routes/judges/runs/[runId]/+page.ts` — 308 → `/sessions/labelling/accuracy/[runId]`, query kept.
- `routes/judges/runs/[runId]/disagreements/+page.ts` — 308 → `/sessions/labelling/accuracy/[runId]/review`, query kept.
- `routes/judges/disagreements/+page.ts` — 308 → `/sessions/labelling/review`, query kept.

**Review labels query state:** `/sessions/labelling/accuracy/[runId]/review` reads `mode` (`reaction`, the default, or `delivery`; older values fall back to `reaction`), `gold` and `judge` (the label and the judge's answer to filter on, for the tab's question), `agree=1` (include turns where they agree) and `turn`. State changes replace the URL query with these fields, omitting default values. The queue loads the first 500 server matches by turn ID; Previous/Next and `j`/`k` walk that loaded queue.

**Session focus query:** `/sessions/[sessionId]?request={id}` scrolls the session timeline to that request's turn and expands it, with the overview above. Scrolling the page afterwards never rewrites the path to `/requests/{id}`; only the `/sessions/[sessionId]/requests/[requestId]` route writes its own path back.

**Harnesses** (`__harness__/` subdirs): wrappers used by Storybook to mount whole-route stories. Not addressable as URLs.

## 2. Layout Hierarchy

The SPA has a single root layout — no nested `+layout.svelte` files. All shell chrome is rendered there:

```
routes/+layout.svelte                 ← root layout
├── <TopBar>                          shell chrome top (44px): forwarding info, LIVE/PAUSED, refresh
├── <ConnectionBanner>                surfaces WS reconnect / lagged state
├── .app grid:
│   ├── <DestinationNav>              left rail: grouped destination side-nav (260px default, collapsible)
│   └── {@render children()}          ← route-specific content fills the right cell
├── (filter authoring at `/filter/workbench` route — no drawer)
├── <HelpDrawer>                      slide-in right; opened via TopBar "?" icon
├── <CommandPalette>                  ⌘K / Ctrl-K palette
└── <EvictionOverlay>                 only when `?debugEvictions=1`
```

The left rail is `DestinationNav` (`shell/DestinationNav.svelte`), a persistent destination side-nav. It replaced the old `SessionsColumn` session-list rail. Nav groups and items (from `DestinationNav.svelte` as of 2026-09-28):

- **Overview** — Home (`/`), Projects (`/projects`), Sessions (`/sessions`), then indented Labelling (`/sessions/labelling`), Labelling accuracy (`/sessions/labelling/accuracy`) and Review labels (`/sessions/labelling/review`; a run's Review labels page lights it), then Comms (`/comms`). The static `labelling` segment outranks `/sessions/[sessionId]`.
- **Levers** — Filters (`/filter/workbench`), Steering Monitor (`/steering`), Steerers (`/steerers` → redirects to `/steering?tab=fleet`)
- **Insight** — Flagged responses (`/analysis`), Prompts (`/prompts`), Agents (`/agents`), Fleet (`/fleet`), Tools (`/tools`), then indented Agent dispatches (`/diagnostics/agent-dispatches`)
- **System** — Memory (`/memory`), Memory diagnostics (`/diagnostics/memory`), Settings (`/settings`), then six indented sub-items — Steering (`/settings/steering`), Workers (`/settings/workers`), Assistants (`/settings/assistants`), Embedding (`/settings/embedding`), Flagging (`/settings/flagging`), Integrations (`/settings/integrations`) — followed by Help (`/help`)

The Workers settings page links to the Labelling page (Turns and Sessions).

`+layout.ts` declares `ssr = false` and `prerender = false` so the whole app is an SPA fallback. `+layout.svelte` is the sole owner of:

- **Stats polling** (`getStats()` every 5s, paused on `visibilitychange` hidden).
- **WebSocket connection** (`proxyEvents.connect()` from `ws.svelte.ts`).
- **Context providers**: `SessionListStore`, `SessionDetailStore`, `CollapseStore`, `FilterRulesStore`, and the `STATS_KEY` getter.
- **WS event draining**: a `$effect` reads `proxyEvents.totalEvents`, walks the ring from `processedEvents` to head, and dispatches each event into `sessionStore.handleEvent()` + `filterRulesStore.handleEvent()`. `Lagged` frames synthesised separately.
- **Quick-create deep link**: handles `/?createFilterRule=1&prefillValue=...` from `AnomalyRow` by opening the filter drawer pre-populated.
- **Global ⌘K handler**: toggles `shellState.commandPaletteOpen`.

The `.app` grid uses CSS variables `--sessions-w` set from `shellState.sessionsCollapsed` / `shellState.sessionsWidth` so resize/collapse react instantly.

## 3. Component Inventory

Alphabetical. Stories (`*.stories.svelte`), tests (`*.test.ts`), harnesses (`__harness__/`), and fixtures (`__fixtures__/`) are omitted; they are not runtime components.

### `lib/components/` (top level)

| Component | File | Key props | Renders |
| --- | --- | --- | --- |
| `BodyViewer` | `BodyViewer.svelte` | `payload: BodyPayload`, `language?` | Full-bytes body view with code highlighting; no slicing. |
| `CacheEfficiency` | `CacheEfficiency.svelte` | `usage: TokenUsage` | Cache hit ratio + tokens donut. |
| `CommandPalette` | `CommandPalette.svelte` | none (reads `shellState`) | ⌘K palette: fuzzy-match across sessions/routes/actions. |
| `CopyIconButton` | `CopyIconButton.svelte` | `text: string` | Copy-to-clipboard icon button. |
| `CostBreakdown` | `CostBreakdown.svelte` | `usage`, `model` | Token cost estimation. |
| `DetailHeader` | `DetailHeader.svelte` | `detail: RequestDetail` | Title row of the request-detail column. |
| `DetailSkeleton` | `DetailSkeleton.svelte` | none | Loading placeholder for the detail column. |
| `DiagnosticsCard` | `DiagnosticsCard.svelte` | `diagnostics: Diagnostic[]` | Per-finding category badges with operator-facing advice for one request; mounted in `RequestDrawer`. |
| `EmptyStateOnboarding` | `EmptyStateOnboarding.svelte` | `onOpenHelp` | Zero-traffic hero with help CTA. |
| `EvictionOverlay` | `EvictionOverlay.svelte` | none | Debug overlay subscribed to six eviction sources. |
| `HeadersTable` | `HeadersTable.svelte` | `headers: Array<[string, string]>` | Full HTTP header table — every byte rendered. |
| `HelpDrawer` | `HelpDrawer.svelte` | `stats?` | Slide-in help drawer; mirrors `/help` content. |
| `HelpHint` | `HelpHint.svelte` | `slug: string`, `term?`, `definition?`, `ariaLabel?` | Inline `?` hint linking to `/help/glossary#<slug>`. |
| `IdentifiersSection` | `IdentifiersSection.svelte` | `detail` | Upstream request/message id, session id, project. |
| `LoadErrorBanner` | `LoadErrorBanner.svelte` | `error: string \| null`, `onRetry?` | Inline error with retry CTA. |
| `MessageContentRenderer` | `MessageContentRenderer.svelte` | `content` blocks | Renders typed assistant/user content blocks. |
| `MetricsStrip` | `MetricsStrip.svelte` | `detail` | Latency / TTFB / tokens row above the band. |
| `ParametersSection` | `ParametersSection.svelte` | `params: RequestParams` | Inline section: model params + thinking config. |
| `ParamsCard` | `ParamsCard.svelte` | `params` | Standalone card variant of `ParametersSection`. |
| `PinToggle` | `PinToggle.svelte` | `pinned: boolean`, `onToggle` | Pin/unpin button (session or project). |
| `PromptComposition` | `PromptComposition.svelte` | `composition: PromptComposition` | Read-only composition viewer. |
| `RequestBodyCard` | `RequestBodyCard.svelte` | `requestId: number` | Lazy request-body fetch + viewer. |
| `RequestErrorCard` | `RequestErrorCard.svelte` | `detail` | Error-status rendering for failed requests. |
| `ResponseBodyCard` | `ResponseBodyCard.svelte` | `requestId: number` | Lazy response-body fetch + viewer (incl. SSE accumulator). |
| `SectionCard` | `SectionCard.svelte` | `title`, `ariaLabel?`, `headerActions?` snippet, `collapsible?`, `sectionKey?`, `defaultCollapsed?`, `children` snippet | Collapsible card; collapse state read from `CollapseStore` context. |
| `StatusPill` | `StatusPill.svelte` | `status: StatusKind` | Pill rendering of derived status (`Pending`/`Streaming`/`Complete`/`Error: ...`). |
| `SystemPromptBreakdown` | `SystemPromptBreakdown.svelte` | `blocks: SystemBlocksProjection` | Per-block view of `system` array with composition match badges. |
| `SystemPromptDetails` | `SystemPromptDetails.svelte` | `detail` | Wrapper that picks `SystemPromptBreakdown` vs raw. |
| `SystemPromptVisibilitySignal` | `SystemPromptVisibilitySignal.svelte` | `detail` | "composition applied" badge with deep-link to `/prompts/[sha]`. |
| `ToolNamesChips` | `ToolNamesChips.svelte` | `names: string[]` | Chip list of declared / used tool names. |
| `Tooltip` | `Tooltip.svelte` | `text`, slot | Hover tooltip primitive. |
| `TurnDeltaHeader` | `TurnDeltaHeader.svelte` | `delta`, `predecessorId` | Header for delta rows (added/removed messages). |

### `lib/components/shell/` (shell chrome)

| Component | File | Key props | Renders |
| --- | --- | --- | --- |
| `BandSeparator` | `shell/BandSeparator.svelte` | `delta` | Separator chrome between request bands. |
| `ColumnResizer` | `shell/ColumnResizer.svelte` | `onResize`, `min`, `max` | Drag handle that mutates column width via `shellState`. |
| `ConnectionBanner` | `shell/ConnectionBanner.svelte` | none | Reconnect/lagged banner pulled from `proxyEvents.state`. |
| `DestinationNav` | `shell/DestinationNav.svelte` | none (reads `shellState`, `page`) | Left rail: grouped destination side-nav with collapsible/resize plumbing. Replaced `SessionsColumn` as the active left-rail component. |
| `NewMessagesAffordance` | `shell/NewMessagesAffordance.svelte` | `count`, `onClick` | "N new messages" jump-to-bottom button. |
| `RequestBand` | `shell/RequestBand.svelte` | `summary`, `detail?` | One request as a stacked band with inline sections. |
| `RequestDetailColumn` | `shell/RequestDetailColumn.svelte` | reads URL params | The 4th column: identity bar + `SessionTimeline` + `RequestDrawer`. |
| `RequestDrawer` | `shell/RequestDrawer.svelte` | reads `shellState.drawerKind` / `drawerBandId` | Slide-in heavy-bytes drawer (request/response/events/findings). |
| `SessionOverview` | `shell/SessionOverview.svelte` | `now`, `timeline` snippet | `/sessions/[id]` landing dashboard; embeds the timeline below the seam. |
| `SessionsColumn` | `shell/SessionsColumn.svelte` | `now` | Legacy session-list column (Storybook harness only; no longer mounted by the layout). |
| `SessionTimeline` | `shell/SessionTimeline.svelte` | `detailStore`, `now` | Unified session-wide band timeline with windowing. |
| `StatusIndicators` | `shell/StatusIndicators.svelte` | `stats` | Top-bar status pills (persistence on/off, etc.). |
| `ToolUsesPanel` | `shell/ToolUsesPanel.svelte` | `sessionId`, `now` | Side panel: full session tool-use table (paginated). Mounted by `SessionOverview`. |
| `TopBar` | `shell/TopBar.svelte` | `stats`, `onRefresh` | Top chrome: endpoints, LIVE/PAUSED, refresh/pause buttons. |

### `lib/components/session-flow/` (Flow view, `?view=flow` on `/sessions/[sessionId]`)

`SessionFlow.svelte` composes one framed instrument (`.edge.mech-group`, the same
recipe `SessionOverview`'s mechanism group uses one row above it on the same
route) holding six grouped `SectionCard` members, in reading order, with a
hairline seam between them and no other content between — never a stack of
separately bordered cards:

| # | Member | File | Renders |
| --- | --- | --- | --- |
| 1 | Session flow | `FlowHeaderBand.svelte` | Static identity header: session id, peer (if any), start date, agent count, the coordinator's title, a `Timeline ↗` link back to the Requests tab, and — only while stale — a `chip-warning` STALE badge whose hover tooltip names the reasons. |
| 2 | Timeline | `FlowPlayerBand.svelte` | The transport (`FlowTransport.svelte`: skip/rewind/play-pause/fast-forward/skip, follow-live, speed, the REPLAY/LIVE mode badge, the playhead clock), a seek `Slider`, the five role lanes (`FlowLanes.svelte`, from `laneModel.ts`), and the recorded-moments row (`FlowMarkers.svelte`, from `markers.ts`). |
| 3 | Board | `SessionFlowBoard.svelte` | The FUNK Flow Instrument board: four `BoardColumn`s (SESSION, AGENTS, MIKE · PROXY · THE BOUNDARY, OUTSIDE THE BOUNDARY) laid out by `boardLayout.ts`, chips (`shared/board/NodeChip.svelte`) routed by `board/router.ts`, lit by `traceModel.ts`'s window at the playhead. Scrolls horizontally (`overflow-x: auto`) below its natural 1372px width. Header line reads `<columns> COLUMNS · <n> REQUESTS IN FLIGHT · <m> TOOL CALLS OPEN · <e> ERROR` from `counts.ts`'s `buildCounts`. |
| 4 | Filter | `FlowFilterBand.svelte` | The legend/filter toggle row (`shared/LegendFilter.svelte`, vocabulary in `legendEntries.ts`) — every entry toggles a WHO identity, a trace kind, or a node-ring state on the board above; sits under the board, not beside it. |
| 5 | Selected | `FlowDetailBand.svelte` | What's currently selected: one title line, one explanation line with complete in-flight id lists (never a slice), an `Open evidence ↗` action, and — when the selection is a followable agent worker — a secondary follow toggle. |
| 6 | Evidence | — (composed directly in `SessionFlow.svelte`, `SectionCard` collapsed by default) | The full recorded-detail panel: `FlowEvidencePanel.svelte` (per-selection dl + causal-evidence traversal), `FlowActivityList.svelte` (paginated phase list), the relocated brush-window numeric inputs, and the accessible actors/coverage/browser-observation disclosures. `Open evidence ↗` (member 5) expands this member programmatically via the shared `CollapseStore`. |

Supporting, non-member files: `sessionFlowStore.svelte.ts` (`SessionFlowStore`,
`FlowLiveMark`, `FLOW_COMPONENTS`), `playback.svelte.ts` (`createFlowPlayback`,
`FlowPlayback`), `projection.ts` (`sceneAt`, `normalizeFlowDataset`),
`grouping.ts` (`buildAgentIdentities`), `boardLayout.ts` + `board/router.ts` +
`board/types.ts` (the board's own layout/routing/trace vocabulary),
`__fixtures__/fleet.ts` (`fleetDataset`, `peakMs` — the Storybook + route-story
fixture: 24 agents, a peer, sidecars, 8 open tool calls, 9 Mike service
roles). Selection never changes the playhead; the six members share one
`SessionFlowProps` interface — see `SessionFlow.svelte`'s module script, kept
stable so `+page.svelte` can import the component lazily as
`Component<SessionFlowProps>`.

### `lib/components/live/` (live system panel)

| Component | File | Renders |
| --- | --- | --- |
| `ActivityFeed` | `live/ActivityFeed.svelte` | Rolling request activity rows. |
| `HealthStrip` | `live/HealthStrip.svelte` | Health KPIs (uptime, error rate). |
| `ResourceGauges` | `live/ResourceGauges.svelte` | Memory / RSS gauges driven by `MemoryTick`. |
| `Sparkline` | `live/Sparkline.svelte` | Generic sparkline primitive. |
| `SystemLivePanel` | `live/SystemLivePanel.svelte` | Wrapper for all four live sections; consumed by `/`. |
| `ThroughputKpis` | `live/ThroughputKpis.svelte` | Requests/sec, tokens/sec sparklines. |

### `lib/components/message-view/`

| Component | File | Renders |
| --- | --- | --- |
| `CodeBlock` | `message-view/CodeBlock.svelte` | Syntax-highlighted code block. |
| `ReminderCard` | `message-view/ReminderCard.svelte` | `<system-reminder>` block card. |
| `ReminderRow` | `message-view/ReminderRow.svelte` | One row inside `ReminderCard`. |
| `ReminderSubgroup` | `message-view/ReminderSubgroup.svelte` | Collapsible subgroup inside `ReminderCard`. |
| `SubRow` | `message-view/SubRow.svelte` | One sub-row inside a user/tool message. |
| `SubRowDetail` | `message-view/SubRowDetail.svelte` | Expanded detail body for a sub-row. |
| `SystemRemindersCard` | `message-view/SystemRemindersCard.svelte` | Card listing all system reminders for a turn. |
| `Tag` | `message-view/Tag.svelte` | Generic tag chip (variants in `Tag.variants.ts`). |
| `UserMessageCard` | `message-view/UserMessageCard.svelte` | One user turn card. |
| `UserTurnView` | `message-view/UserTurnView.svelte` | Full user-turn view (header + body + sub-rows). |

Pure helpers in this dir: `messageWalker.ts`, `parseSystemPrompt.ts`, `stripChrome.ts`.

### `lib/components/prompts/`

| Component | File | Renders |
| --- | --- | --- |
| `AnomalyRow` | `prompts/AnomalyRow.svelte` | One row in the `/prompts` anomalies table; emits create-filter-rule deep link. |
| `BlockCard` | `prompts/BlockCard.svelte` | One `system[N]` block; editable on `/prompts/new`. |
| `ClassChip` | `prompts/ClassChip.svelte` | Composition class identity chip (`prompt_sha256` short + label). |
| `CompositionRow` | `prompts/CompositionRow.svelte` | One row in the classes table. |
| `CompositionStatsHeader` | `prompts/CompositionStatsHeader.svelte` | KPI strip for the composition detail page. |
| `PromptCompositionDiff` | `prompts/PromptCompositionDiff.svelte` | Drift view: composition vs request `system_blocks`. |
| `PromptKpiStrip` | `prompts/PromptKpiStrip.svelte` | 4-cell KPI strip at the top of `/prompts`. |

### `lib/components/filter/`

| Component | File | Renders |
| --- | --- | --- |
| `FilterWorkbench` | `filter/FilterWorkbench.svelte` | The filter-authoring page body (`/filter/workbench`): a 52px toolbar (model/class/version, view tabs, prompt size, Read only), a collapsible outline, the prompt pane, and a side panel (edits above, assistant below) split by draggable dividers. |
| `WorkbenchDivider` | `filter/WorkbenchDivider.svelte` | One resizable splitter (pointer + arrow-key), used for all three dividers; widths persist via `workbenchLayout.ts`. |
| `WorkbenchNavigator` | `filter/WorkbenchNavigator.svelte` | The outline pane: an "In this prompt" heading list (jump links, a marker on any section with edits) built from the paragraph splitter, then model, prompt class, Claude Code release history, other observed models, back-link to the request. |
| `WorkbenchSourcePane` | `filter/WorkbenchSourcePane.svelte` | The prompt pane, three lenses: As sent (verbatim), With my edits (word/sentence diff of source vs. dry-run output mapped onto PARAGRAPHS, not whole blocks — struck removed, green-washed added, blue when previewing an assistant proposal), Final (dry-run output). Each block is split into paragraphs at blank lines, markdown headings, and list runs (`blockParagraphs.ts`); a changed paragraph's margin names the edit(s) attributed to it (plain title, no rule id) and a hover "Ask about this" quotes that paragraph only. "Show in prompt" scrolls to and highlights one paragraph. |
| `WorkbenchRuleStack` | `filter/WorkbenchRuleStack.svelte` | The "Your edits" section: a flat list of `WorkbenchRuleCard` rows, an "only this prompt" filter, inline "Add rule" (`FilterRuleForm` create mode) + empty state. |
| `WorkbenchRuleCard` | `filter/WorkbenchRuleCard.svelte` | One edit row: plain title (`rule.title` or derived), where it lands, an enable switch; opens in place to before/after, reach, and "Reword with assistant" / "Show in prompt" / "Edit by hand" (reveals `FilterRuleForm` inline). |
| `WorkbenchAssistantPanel` | `filter/WorkbenchAssistantPanel.svelte` | The prompt-assistant chat: streamed messages over `POST /api/assistant/conversations/:id/messages` (SSE), proposal cards (Preview in prompt / Apply), a composer with a quote chip and starter questions; says so in one line when the service is off or unreachable. |
| `AddRulePopover` | `filter/AddRulePopover.svelte` | Inline popover to create a rule from a block. |
| `FilterRuleForm` | `filter/FilterRuleForm.svelte` | Create/edit form for a rule, including its `title`. |

Pure helpers: `filter/rule-matching.ts` (substring/xml-tag client-side match preview, plain-title derivation, "where it lands", before/after, paragraph attribution), `filter/blockParagraphs.ts` (splits a block into display paragraphs at blank lines/headings/list runs; builds the outline's heading list), `filter/workbench.ts` (rule-match counting / ordering), `filter/promptDiff.ts` (word/sentence-granularity text diff), `filter/workbenchLayout.ts` (pane-size persistence). The assistant service client lives at `$lib/api/assistant.ts` (health/conversations/SSE reader), matching `services/prompt-assistant/src/types.ts`'s wire contract (`edit_id`/`FilterRuleDto.id` are both strings; a message's `proposals` is a plural array).

Filter rules are DB-only state authored at runtime via these components and the `*_filter_rule` API/MCP tools; there is no bootstrap-file filter surface.

### `lib/components/help/`

| Component | File | Renders |
| --- | --- | --- |
| `GuideLayout` | `help/GuideLayout.svelte` | Wrapper used by every `/help/guides/*` page. |

### `lib/components/eval/`

| Component | File | Renders |
| --- | --- | --- |
| `EvalRunSummary` | `eval/EvalRunSummary.svelte` | Compact run/metric line; expanding it reveals configuration, bundle provenance and equivalents, costs, item counts and corrected-class precision/recall. |
| `EvalReviewQueue` | `eval/EvalReviewQueue.svelte` | Selectable turn queue with disagreement, reviewed and changed-gold indicators; asks users to narrow filters when matches exceed the loaded limit. |
| `EvalReviewPane` | `eval/EvalReviewPane.svelte` | Operator prompt, assistant output and next genuine message alongside `EvalVerdictChoice`; session/request links and collapsible label history. Distinguishes missing evidence from empty turns. |
| `EvalVerdictChoice` | `eval/EvalVerdictChoice.svelte` | Recorded labels against the judge's answer, their supporting evidence and current-label changes; “Label is correct” / “Judge is correct” actions, reviewer note and a disclosure for custom per-axis choices. |
| `EvalReviewDecision` | `eval/EvalReviewDecision.svelte` | Reaction/delivery radio choices inside `EvalVerdictChoice` when neither answer is wholly correct; saves the selected pair. |
| `EvalChoiceGroup` | `eval/EvalChoiceGroup.svelte` | Reusable choice-button group for decided-state, gold-label, judge-label and severity filters. |
| `LabelHistoryList` | `eval/LabelHistoryList.svelte` | Label history with provenance, withdrawal actions and confirmation of current relayed labels as gold. |

Pure helpers: `eval/evalReview.ts` provides filter/query conversion, reviewed-item filtering, queue navigation, keyboard shortcuts, verdict-choice derivation, disagreement counts, provenance wording, grouped label evidence and `formatPercent` / `formatKappa`. `Alt+G` keeps the recorded label, `Alt+J` takes the judge's answer, `j`/`k` navigates, `r`/`d` opens custom axis choices and `Ctrl/⌘+Enter` saves those choices. The `accuracy(axis)` helper lives in `$lib/api/evalRuns.ts`.

### `lib/components/ui/` (shadcn-svelte primitives)

Wrapped Bits UI primitives (Svelte port of Radix UI). One folder per primitive with sub-parts and an `index.ts` re-export:

- `ui/button` — `button.svelte`
- `ui/dialog` — `dialog`, `dialog-trigger`, `dialog-content`, `dialog-overlay`, `dialog-portal`, `dialog-header`, `dialog-footer`, `dialog-title`, `dialog-description`, `dialog-close`
- `ui/popover` — `popover`, `popover-trigger`, `popover-content`, `popover-portal`, `popover-close`
- `ui/separator` — `separator.svelte`
- `ui/sheet` — `sheet`, `sheet-trigger`, `sheet-content`, `sheet-overlay`, `sheet-portal`, `sheet-title`, `sheet-description`, `sheet-close`
- `ui/tabs` — `tabs`, `tabs-list`, `tabs-trigger`, `tabs-content`
- `ui/tooltip` — `tooltip`, `tooltip-provider`, `tooltip-trigger`, `tooltip-content`, `tooltip-portal`

## 4. Svelte Stores

All under `frontend/src/lib/stores/`. Each `.svelte.ts` file uses Svelte 5 runes (`$state`, `$derived`, `$effect`) at module scope.

| Store | File | Holds | Owners / consumers |
| --- | --- | --- | --- |
| `SessionListStore` | `sessions.svelte.ts` | Paginated `SessionSummary[]`, WS event overlay map (`wsEventByRequestId`), throttle timers. | Layout instantiates one and publishes via `SESSION_LIST_STORE_KEY`. Consumers: `SessionsColumn`, `/+page.svelte`. |
| `SessionDetailStore` | `sessionDetail.svelte.ts` | One `SessionDetail` cache with in-flight dedup. | Layout instantiates one; consumers: `/sessions/[id]/+page.svelte` (loads), `SessionTimeline`, `SessionOverview`, `RequestDetailColumn`. |
| `RequestDetailStore` | `requestDetail.svelte.ts` | Process-global `Map<id, RequestDetail>` LRU (cap 50); buffers pre-fetch WS events; merges TextDelta / ToolUseStart / etc. into the cached entry. | Created per consumer via `createRequestDetailStore(id)`; used by `RequestDetailColumn`. |
| `RequestListStore` | `requests.svelte.ts` | `RequestSummary[]` for legacy list views. | Created per consumer; currently only stubs / tests use it post Phase 2.8. |
| `CollapseStore` | `collapseState.svelte.ts` | `Set<string>` of collapsed `SectionCard` keys, persisted to localStorage. | Layout instantiates one; every `SectionCard` reads via context. |
| `FilterRulesStore` | `filterRules.svelte.ts` | `FilterRuleDto[]`, mutation warnings, initialised flag. | Layout instantiates one; consumers: `FilterWorkbench`, `WorkbenchRuleStack`, `AddRulePopover`, per-block badges. |
| `PromptCompositionsStore` | `promptCompositions.svelte.ts` | `PromptCompositionListItem[]` + per-page mutation state. | Page-local in `/prompts/+page.svelte` (V1); reserved key `PROMPT_COMPOSITIONS_STORE_KEY` for V2 layout-level. |
| `DiagnosticsStore` | `diagnostics.svelte.ts` | `DiagnosticStats` aggregate counters + 10s poll. | `/analysis` page. |
| `FindingsStore` | `findings.svelte.ts` | Flat `Finding[]` + `groupFindings` reducer (category → session → request). | `/analysis` page. |
| `shellState` | `shellState.svelte.ts` | Module-singleton: `filterText`, `paused`, `activeNav`, drawer flags, column widths/collapse, drawer kind/bandId. Persisted column state in localStorage. | Imported directly by `TopBar`, `RequestDrawer`, `HelpDrawer`, etc. |
| `timelineScroll` | `timelineScroll.svelte.ts` | Module-singleton: scroll mode (`stuck-to-bottom` / `scrolled-up`), `newCount`, `activeBandId`. | `SessionTimeline` writes, `RequestDetailColumn` reads. |
| `bodyCache` | `bodyCache.svelte.ts` | Bounded LRU `Map<"${id}:${kind}", BodyPayload>` (request/response/request_sent). | `RequestBodyCard`, `ResponseBodyCard`. |
| `bodyFetcher` | `bodyFetcher.svelte.ts` | Shared state machine: peek `bodyCache`, fetch on miss, retry, stale-fetch guard. | `RequestBodyCard`, `ResponseBodyCard`. |
| `bandUnmount` | `bandUnmount.svelte.ts` | Emits `BandUnmount` events when `SessionTimeline` evicts from the rendered band window. | Subscribed only by `EvictionOverlay`. |
| `deltaFallback` | `deltaFallback.ts` | Pure utility (not a store) that derives a delta when the backend lacks one. | `TurnDeltaHeader`. |

`liveSystem` (`$lib/state/liveSystem.svelte.ts`) and `memory` (`$lib/state/memory.svelte.ts`) are state modules outside `stores/`. They derive panel-shaped views from the WS `MemoryTick`/`MemoryShed` stream — consumed by the home page (`SystemLivePanel`) and `/diagnostics/memory`.

### Context keys

`$lib/context-keys.ts` exports six `Symbol()`s used with `setContext`/`getContext`:

- `SESSION_LIST_STORE_KEY`
- `SESSION_DETAIL_STORE_KEY`
- `COLLAPSE_STORE_KEY`
- `FILTER_RULES_STORE_KEY`
- `PROMPT_COMPOSITIONS_STORE_KEY` (reserved for V2)
- `STATS_KEY` — typed as `{ readonly value: StatsResponse | null }`, getter-wrapped so consumers track `$state` reactivity.

## 5. WebSocket Integration

`$lib/ws.svelte.ts` exports a `ProxyEventStream` class plus a module-singleton `proxyEvents`.

### Connection

- `buildWsUrl()` derives `ws[s]://${window.location.host}/api/ws`. In dev, vite proxies through to `:9989`; in production the SPA and inspect API share the same origin.
- `proxyEvents.connect()` is called once from `+layout.svelte`'s `onMount`. Idempotent: a no-op if a socket is already open.
- Reconnect: exponential backoff, max 5 attempts, capped at 30s. Delay = `500ms * 2 ** attempts` → schedule: `500ms, 1s, 2s, 4s, 8s` (clamped to 30s).
- Connection state exposed via `state: 'connecting' | 'connected' | 'reconnecting' | 'disconnected'` (consumed by `ConnectionBanner`).

### Event delivery

- Ring buffer of `MAX_EVENTS = 1000`. Reactive `totalEvents: number` is monotonic; consumers track a `processedEvents` cursor and call `at(i)` to read the slot.
- Pre-W1.20-B used to allocate a new array per push; the ring avoids GC churn.
- Layout `+layout.svelte` owns the single drain `$effect`: when `totalEvents` advances it walks the unread window and forwards each frame to `sessionStore.handleEvent()` + `filterRulesStore.handleEvent()`. Per-page stores (e.g. `RequestDetailStore`) drain themselves from their own `$effect`.

### `ProxyEvent` variants (from `types.ts`)

Discriminated by `type`:

- Per-request payload (carry `id: number`): `ModelConfirmed`, `TextDelta`, `ThinkingDelta`, `ToolUseStart`, `ToolInputDelta`, `ToolUseEnd`, `ResponseComplete`, `ResponseError`, `UsageReceived`, `DiagnosticReady`, `StreamingStarted`.
- `RowChanged` — Postgres `LISTEN/NOTIFY` signal carrying `table: 'requests' | 'sessions' | 'filter_rules' | 'prompt_compositions'`, `id: string` (TEXT/UUID/BIGINT as string), `session_id: string | null`, `op: 'INSERT' | 'UPDATE' | 'DELETE'`. List stores treat this as the authoritative refetch trigger.
- `MemoryShed` — operator-visible cap-firing event with `path`, `reason`, `bytes`, `limit_bytes`, `request_id`, `session_id`, `note`.
- `MemoryTick` — per-tick in-process byte breakdown + RSS + DB pool + broadcast stats; flattened with full `MemoryTickEvent` fields.
- `Lagged` — synthetic frame emitted by `api/ws.rs` when the broadcast receiver falls behind; payload `skipped: number`.

`ProxyEventWithId` is a type narrowing helper for routing by request id.

## 6. API Client (`$lib/api.ts`)

One typed function per `/api/*` endpoint. Internal helpers: `getJson`, `postJson`, `patchJson` — all parse a `{ error: string }` envelope on non-2xx and throw `ApiError(message, status, url)`.

Path strategy: relative URLs only. In dev they hit vite's proxy → `:9989`; in production they hit the same origin.

### Function categories

**Health & stats**
- `getHealth()` → `/api/health`
- `getStats()` → `/api/stats`

**Requests**
- `listRequests(params?)` → `/api/requests` (paginated + optional `prompt_sha256` drilldown filter)
- `getRequest(id)` / `getRequestDetail(id)` → `/api/requests/{id}`
- `getRequestSummary(id)` → `/api/requests/{id}/summary`
- `getRequestBody(id)` → `/api/requests/{id}/body/request`
- `getResponseBody(id)` → `/api/requests/{id}/body/response`
- `getRequestSentBody(id)` → `/api/requests/{id}/body/request_sent`
- `getStreamChunks(id)` → `/api/requests/{id}/stream_chunks`
- `getAccumulatedText(id)` — derived: filters `delta_type === 'text'` chunks from `getStreamChunks` and joins by `seq_ord` (the `accumulated_text` column was dropped by migration `20260513000001`).

**Sessions**
- `listSessions(params?)` → `/api/sessions` (paginated + optional pg_trgm `search`)
- `getSession(id, params?)` → `/api/sessions/{id}` (paginated requests embedded)
- `listSessionToolUses(id, params?)` → `/api/sessions/{id}/tool_uses` (paginated, full-fidelity rows)
- `patchSessionPin(id, pinned)` → `PATCH /api/sessions/{id}/pin`
- `patchProjectPin(path, pinned)` → `PATCH /api/projects/{path}/pin`

**Diagnostics (Phase 3)**
- `getDiagnosticsStats()` → `/api/diagnostics/stats`
- `listFindings(params?)` → `/api/diagnostics/findings` (legacy `Finding[]` shape)
- `listFindingsPaginated(params?)` → `/api/diagnostics/findings?total=true` (envelope with `total`)

**Filter rules (Phase 4)**
- `listFilterRules()` → `/api/filter/rules`
- `createFilterRule(body)` → `POST /api/filter/rules`
- `patchFilterRule(id, patch)` → `PATCH /api/filter/rules/{id}`
- `deleteFilterRule(id)` → `DELETE /api/filter/rules/{id}` (handles both 204 and 200-with-warnings envelopes)

**Prompt compositions**
- `listPromptCompositions()` → `/api/prompts/compositions`
- `getPromptComposition(id)` / `getPromptCompositionBySha256(sha)` → composition detail
- `createPromptComposition(body)` / `patchPromptComposition(id, patch)` / `deletePromptComposition(id)`
- `listPromptAnomalies()` → `/api/prompts/anomalies`
- `getCompositionStats(promptSha256)` → composition usage stats
- `getCompositionRevisions(id)` → edit history
- `exportPromptCompositions()` / `importPromptCompositions(payload)` → bulk JSON envelope

**Agents (Phase 8)**
- `listAgents()` / `getAgent(id)` / `patchAgent(id, patch)` / `deleteAgent(id)`

**Volatile-line registry**
- `listVolatileLines()` / `createVolatileLine(body)` / `patchVolatileLine(id, patch)` / `deleteVolatileLine(id)`

**Eval runs (`$lib/api/evalRuns.ts`)**

- `listEvalRuns(q?)` → `GET /api/eval/runs`; accepts `role`, `status` and `limit` and returns `{ items, total, limit, offset }`. The HTTP endpoint supports offsets, but this helper does not expose them.
- `getEvalRun(id)` → `GET /api/eval/runs/{id}` (run detail, including its nullable report).
- `listEvalRunItems(runId, q)` → `GET /api/eval/runs/{id}/items`; accepts disagreement, label, correctness, severity, reviewed and outcome filters, includes, limit and offset.
- `getEvalReviewDetail(runId, turnId)` → `GET /api/eval/runs/{id}/items/{turn_id}/review` (item, evidence context, frozen run gold, current labels and reviews).
- `getOperatorTurnSpan(turnId)` → `GET /api/operator-turns/{id}`; reads session/request identifiers and request-number bounds for the review pane's link to the captured turn. Failure leaves that optional link unavailable without blocking the review.
- `getEvalBundle(id)` / `listTurnJudgeBundles()` → `GET /api/eval/bundles/{id}` / `GET /api/eval/bundles?role=turn_judge&limit=200`.
- `accuracy(axis)` — derives `agreement / n` from an axis report; returns `null` when the report is absent or `n` is zero.

**Eval labels (`$lib/api/eval.ts`)**

- `getEvalVocabulary()` → `GET /api/eval/vocabulary`; `listEvalLabels(subject)` → `GET /api/eval/labels` with turn or session-end subject filters.
- `postGoldLabels(subject, labels, note, context, evalRunId?)` → `POST /api/eval/labels`. The review page passes `context: 'eval_review'` and the run ID; writes append gold labels with review provenance without rewriting the run's stored gold or metrics.
- `withdrawLabel(id)` / `confirmRelayedLabel(id)` → `POST /api/eval/labels/{id}/withdraw` / `POST /api/eval/labels/{id}/confirm`.

**Settings (`$lib/api/settings.ts`)**
- `getFeatures()` / `patchFeature(key, enabled)` → master feature-toggle catalogue
- `getSettings()` / `patchSetting(key, value)` → fine-grained settings catalogue (env-flag-aware)
- `getSyncStatus()` → background resync worker status (`/settings/workers`)
- `verifyOpenrouterKey()` → validates a steerer-supplied OpenRouter key

**Steering (`$lib/api/steering.ts`)**
- `listSteerers()` / `getSteerer(id)` / `createSteerer(body)` / `patchSteerer(id, patch)` / `deleteSteerer(id)` — steerer fleet CRUD
- `enrolSession(...)` / `disenrolSession(...)` / `getSessionEnrolments(...)` — per-session steerer enrolment
- `getSteeredRequest(id)` / `getSessionSteering(sessionId)` / `revertIntervention(...)` — steered-turn inspection + revert
- `getVerifierCounts(sessionId)` / `listModels()` / `getSteeringVocabulary()` / `assistSteerer(body)` — supporting lookups

### Pagination pattern

`{offset, limit}` query params on every listable endpoint. Backend echoes `offset`/`limit`/`total_count` (or `total_requests`) in the response envelope so the SPA can loop. Caller decides — no implicit fetch-all.

### Error handling

- Every non-2xx → `ApiError(message, status, url)`. `status` lets callers branch on 404, 503, 409, etc.
- Network failures surface as native `TypeError` from `fetch`; callers wrap in try/catch.
- DELETE endpoints normalise 204 (no content) and 200 (with warnings) into one shape so callers branch on warnings only.

### Memory inspection (`$lib/api/memory.ts`)

Separate small module: `getMemoryBreakdown()` → `/api/diagnostics/memory`. Used by `/diagnostics/memory` and `liveSystem.svelte.ts` as a REST fallback when WS is down.

## 7. TypeScript Types (`$lib/types.ts`)

Wire-format types mirroring Rust DTOs. Source files cross-referenced in the header comment: `api/handlers.rs`, `store/models.rs`, `types.rs`, `api/ws.rs`.

### Most important types

| Type | Mirrors | Notes |
| --- | --- | --- |
| `RequestSummary` | `RequestSummary` in `api/handlers.rs` | `GET /api/requests` row. Includes `turn_type`, `turn_origin`, `subagent_persona`, `human_prompt` (full, no slice). |
| `RequestDetail` | `InterceptedRequest` (flattened) | `GET /api/requests/{id}`. **Body bytes NOT included** — fetched lazily via the body endpoints. Carries `system_blocks`, `composition_applied`, `filter_modified`, `firing_rule_ids`, `diagnostics`. |
| `BodyPayload` | Lazy body response | `{ text: string \| null, byte_size: number, has_body: boolean }`. `text === null` when bytes are non-UTF-8. |
| `RequestStreamChunksPayload` / `StreamChunk` | `RequestStreamChunksPayload` | One row per persisted SSE `content_block_delta`. `delta_type ∈ {'text', 'thinking', 'input_json'}`. |
| `SessionSummary` | `SessionSummary` | List row; computed live per request. Includes `pinned`, `title`, `avg_latency_ms`. |
| `SessionDetail` | `SessionDetail` | `summary` + chronological `requests`. Pagination metadata when caller passed `offset`/`limit`. |
| `ToolUseDto` / `ToolUsesResponse` | `ToolUseDto` | One row per `tool_uses` table entry; `input` is the FULL JSONB blob, never sliced. |
| `ToolResultDto` / `RequestToolResultsResponse` | `ToolResultDto` | FK-shaped post Phase 11; matcher is `tool_results.tool_use_id → tool_uses(tool_id)`. |
| `ToolErrorRow` / `ToolErrorsResponse` | `ToolErrorRow` | Cross-request feed of `is_error: true` results. |
| `OperatorTurnDto` / `OperatorTurnOutcomeSummary` | `OperatorTurnDto` | Operator-turn ledger row; aggregates from the read-side `operator_turn_aggregates_v` view. |
| `AgentDispatchDto` | `AgentDispatchDto` | Sub-agent dispatch row joined to `tool_uses` + `agents`. |
| `SystemBlock` / `SystemBlocksProjection` | Server-computed projection | Per-block view of the request `system` array with composition match badges. |
| `PromptComposition` / `PromptCompositionListItem` / `PromptCompositionPatch` | Composition CRUD | Includes `version`, `mode` (`observe`/`template`/`replace`), `agent_name`, `applied_count_24h`. |
| `CompositionRevision` | Composition edit history | One snapshot per patch. |
| `PromptAnomalyGroup` | Anomaly aggregate row | Distinct `(classification, prompt_sha256, signature_hash)`; carries FULL `request_ids` (never sliced). |
| `PromptSubPattern` | Operator-meaningful classifier | Names like `'unregistered-class'`, `'filter-induced-drop'`; `recommended_action` selects the CTA. |
| `FilterRuleDto` / `FilterRuleCreate` / `FilterRulePatch` | Filter rule CRUD | `kind ∈ {text_substring, xml_tag}`, `action ∈ {drop, replace, append}`, `source ∈ {toml, ui}`. |
| `VolatileLinePattern` | Volatile-line registry | Regex + `applies_to_block_index: 0 | 1 | 2`. |
| `Agent` / `AgentListItem` / `AgentDetail` / `AgentPatch` | Agent registry | `source ∈ {observed, file_backed, dynamic}`. |
| `Diagnostic` / `Finding` / `DiagnosticStats` | Phase 3 analyzer output | `matched_text` is FULL (no cap); `match_range` is `[start, end]` byte offsets. |
| `MemoryTickEvent` / `MemoryShedEvent` | Live system stream | Flattened alongside `type: 'MemoryTick'` / `'MemoryShed'` on the WS wire. |
| `ProxyEvent` / `ProxyEventWithId` | WS frame discriminated union | See WebSocket section above. |
| `StatsResponse` | `/api/stats` | `request_count`, `active_streams`, `uptime_seconds`, `db_enabled?`. |
| `TokenUsage` | Per-request usage | `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`. |

Type discipline: a Rust struct change must be reflected here, but the field names are the JSON wire names (snake_case) — not the Rust idents. Optional fields (`?:`) are used for forward-compat with older backends.

### Eval types (`$lib/api/evalRuns.ts`)

`EvalRunDto`, `EvalRunItemDto` and `EvalRunReportDto` describe run details, reviewed turn items and the nullable run report. `status` is typed as `string`; database values are `running`, `paused_budget`, `complete`, `failed` and `cancelled`. Reaction and delivery axis reports contain `n`, `agreement` and nullable `kappa`, with optional confusion/per-class metrics; accuracy is derived by `accuracy(axis)`. Review evidence and labels are represented by `EvalReviewDetailDto` and `EvalReviewContextDto`. `OperatorTurnSpanDto` is the subset of operator-turn detail used for the session/request link and request-number bounds.

## 8. Build & Dev

- `vite dev` on `:9990` is the default boot path (`npm run start` aliases to `vite dev`).
- `npm run build` produces `frontend/build/` (the static SPA fallback).
- `npm run preview` serves the built output on `:9990` for production-style hosting.
- Storybook (`npm run storybook`) renders every `*.stories.svelte`. Stub modules live in `lib/__storybook-stubs__/` and re-export sanitised versions of `api`, `ws`, and the layout-owned stores so stories never hit the network.
- Self-hosted JetBrains Mono via `@fontsource-variable/jetbrains-mono` imported from `+layout.svelte`.
