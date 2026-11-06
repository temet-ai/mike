---
title: Frontend Component Catalogue
status: active
created: 2026-09-08
updated: 2026-10-08
summary: Every component-name family (Card, Panel, Row, ...) with two or more members, its canonical component, and why every other member is not yet an extension of it. Checked by scripts/rule-checks/component-families.mjs on every frontend gate.
---

# Frontend component catalogue

The frontend groups components by convention, not enforcement — a name ending in `Card`, `Panel`,
`Row` and so on signals "this is shaped like the others with that suffix," but nothing checked that
signal against reality. `scripts/rule-checks/component-families.mjs` does: it groups every
production component under `frontend/src/lib/components/` by filename suffix into the families
below, and fails the frontend gate the moment a family gains a member this file doesn't account
for. `Badge`, `Pill`, `Chip`, `Chips` and `Tag` are grouped as one family, `Badge`, because unit B2
below folded them into one consolidation target, not five unrelated suffixes.

Each family names a **canonical** component — picked on the evidence available today, importer
count first, prop genericity as the tiebreaker — and gives every other member a one-line reason it
is not (yet) an extension of the canonical one. One reason still recurs, for a follow-up unit of the
`frontend-motion-and-consolidation` thread that has not run yet:

- **pending review against SectionCard (unit B4)** — every `*Card` and `*Panel` component gets
  reviewed against `SectionCard`'s expand/collapse shape after Angle A's motion-and-elevation layer
  (units A1-3) lands; the ones that are SectionCard-shaped become a SectionCard usage or variant.

That placeholder is for real, deferred work, not a permanent excuse. The Badge family's own
placeholder, "pending consolidation (unit B2)", is gone from this file because that unit has run —
see the Badge section below for the real state. Every other reason in this file is a real, current
distinction: two components that share a suffix by naming convention but do not share a shape, and
are not expected to merge.

When a new component would extend an existing family, extend the canonical component (a prop or a
variant) instead of adding a file. When it genuinely can't, add it here with a real reason before
the file lands — the gate fails on a member missing from this list.

## Badge

Canonical: `frontend/src/lib/components/shared/Badge.svelte`

Unit B2 folded the badge family into this one base component. It owns every badge/pill/chip/tag
appearance as a closed `BadgeVariant` set (`Badge.variants.ts`, mirroring `Tag.variants.ts`'s
sidecar pattern) and all of the family's styling — the augmented-ui clip treatment previously
duplicated across StatusPill/OutcomeBadge/PromptKindBadge/ToolNamesChips, and ClassChip's bespoke
identity-chip CSS, now live only in `Badge.svelte`'s `<style>` block. Two components rendering the
same look map to the same variant (`chip-active` covers both the deleted OutcomeBadge's "accepted"
and PromptKindBadge's "operator"; `chip-muted` covers OutcomeBadge's "unknown" and PromptKindBadge's
seven harness kinds). `OutcomeBadge.svelte` is deleted outright: two independent checks (`rg`, and
its absence from the `lib/components/index.ts` barrel) confirmed zero importers outside its own
story. `detail/ToolNamesChips.svelte` is deleted for the same reason, found by the same two checks
during this unit's fix round — it was the sixth badge-family member and the only caller of a
`size="compact"` variant option, which is removed along with it (nothing else ever set it).

- `frontend/src/lib/components/skills/SkillStateBadge.svelte` — thin wrapper: maps a learned
  skill's `SkillState` onto its words (`STATE_TEXT` in `lib/api/skills.ts`) and a `BadgeVariant`,
  and renders `<Badge>`; used by the Skills list rows and the skill page.
- `frontend/src/lib/components/message-view/Tag.svelte` — thin wrapper: maps its own `TagVariant`
  domain vocabulary (group/block/hook/boot/event-info/event-warn/catchall/default) onto a
  `BadgeVariant` and renders `<Badge>`. Kept as a separate component, not folded into a re-export,
  because `message-view/index.ts` re-exports `tagVariants`/`TagVariant` directly, untouched by this
  consolidation. (`ReminderCard.svelte` also imported the type directly when this line was first
  written; unit B4 deleted that file for having no render site anywhere in the tree.)
- `frontend/src/lib/components/prompts/ClassChip.svelte` — thin wrapper: maps `isActive` to the
  `identity`/`identity-active` variant and forwards `href` to Badge's polymorphic anchor/span render.
- `frontend/src/lib/components/shared/StatusPill.svelte` — thin wrapper: maps `RequestStatus` to a
  `pill-*` variant; keeps the Tooltip wrap and the in-flight `StreamingIndicator` composition, which
  are behaviour, not styling.
- `frontend/src/lib/components/shared/PromptKindBadge.svelte` — thin wrapper: translates
  `promptKind.ts`'s existing `KIND_CLASS` class-string mapping (left untouched, shared with
  `SessionEventRow`) into a `BadgeVariant`, so a kind added there keeps working without this file
  changing.
- `frontend/src/lib/components/shared/board/NodeChip.svelte` — a PCB-style circuit-board node on
  the FUNK Flow Instrument board (funk-update-2026-09-15.md §3.3): kind-shaped silhouette, an
  identity plate, an icon and a two-line text block, absolutely positioned by a router. Shares
  nothing with `BadgeVariant`'s tag/pill/status vocabulary — its "chip" sense is a schematic
  component footprint, not a status/tag chip — and its own `.edge`/`.edge-4` clip primitive and
  per-kind size table have no `Badge` equivalent to extend.

### Raw badge class users, pending unit B2b

Unit B2 only folded the six named components above into `Badge.svelte`. It left `.status-pill*` and
`.badge-nx`/`.badge-*` declared in `frontend/src/app.css`, and did not touch the following files,
none of which import any of the six — each writes one of those class names as a literal string in
its own markup, fixture data, or classification map. Unit B2b converts these to `Badge` callers and
then moves the badge CSS block out of `app.css` into the base component; it is sequenced after Angle
A releases `app.css` from its own current allowlist.

- `frontend/src/routes/agents/+page.svelte`
- `frontend/src/routes/diagnostics/memory/+page.svelte`
- `frontend/src/routes/help/glossary/+page.svelte`
- `frontend/src/routes/projects/+page.svelte`
- `frontend/src/routes/prompts/+page.svelte`
- `frontend/src/routes/prompts/new/+page.svelte`
- `frontend/src/lib/diagnostics.ts`
- `frontend/src/lib/diagnostics.test.ts`
- `frontend/src/lib/comms/CommsPeerStrip.svelte`
- `frontend/src/lib/comms/CommsTranscript.svelte`
- `frontend/src/lib/comms/deliveryPresentation.ts`
- `frontend/src/lib/comms/deliveryPresentation.test.ts`
- `frontend/src/lib/fleet/FleetBoardRow.svelte`
- `frontend/src/lib/fleet/FleetCanvasNode.svelte`
- `frontend/src/lib/fleet/FleetInspectorPanel.svelte`
- `frontend/src/lib/fleet/graphNormalize.ts`
- `frontend/src/lib/prompts/catalogueRow.ts`
- `frontend/src/lib/components/shared/promptKind.ts`
- `frontend/src/lib/components/memory/ArchivedLessonCard.svelte`
- `frontend/src/lib/components/memory/DecisionActionCard.svelte`
- `frontend/src/lib/components/memory/LessonOutcomeRow.svelte`
- `frontend/src/lib/components/memory/LessonRelationCard.svelte`
- `frontend/src/lib/components/memory/RunsStrip.svelte`
- `frontend/src/lib/components/projects/ProjectDetailPanel.svelte`
- `frontend/src/lib/components/prompts/BlockCard.svelte`
- `frontend/src/lib/components/prompts/PromptCatalogueRow.svelte`
- `frontend/src/lib/components/prompts/PromptCatalogueRow.stories.svelte`
- `frontend/src/lib/components/prompts/PromptClassDetail.svelte`
- `frontend/src/lib/components/sessions/SessionRow.svelte`
- `frontend/src/lib/components/shell/SessionMailboxPanel.svelte`
- `frontend/src/lib/components/shell/SessionMailboxPanel.stories.svelte`
- `frontend/src/lib/components/shell/SteeringBand.stories.svelte`
- `frontend/src/lib/components/shell/SystemPromptCard.svelte`
- `frontend/src/lib/components/detail/BigbroAttribution.stories.svelte`
- `frontend/src/lib/components/detail/SystemPromptBreakdown.svelte`
- `frontend/src/lib/components/detail/SystemPromptVisibilitySignal.svelte`
- `frontend/src/lib/components/detail/TransformationManifestCard.svelte`
- `frontend/src/lib/components/__foundations__/Borders.stories.svelte`
- `frontend/src/lib/components/__foundations__/Brand.stories.svelte`
- `frontend/src/lib/components/__foundations__/KnowledgeVocabulary.stories.svelte`
- `frontend/src/lib/components/__foundations__/Typography.stories.svelte`
- `frontend/src/lib/components/shell/SessionEndChip.svelte` — composes `Badge`: a session's end label, its origin (rule or llm), confidence and failure tags with a link to the full verdict history; a domain wrapper that picks variants, not a new appearance.
- `frontend/src/lib/components/shell/WakeStateChip.svelte` — composes `Badge`: shows a guard nudge's delivery channel and wake state as two chips, active once pushed; no styling of its own.
- `frontend/src/lib/components/shell/TurnVerdictChips.svelte` — composes `Badge` once per axis: a turn verdict's reaction, delivery and source chips, with the variant mapping in `turnVerdictChips.ts`.

## Band

Canonical: `frontend/src/lib/components/home/NowBand.svelte`

Four full-width page sections that share the "Band" name by page-layout vocabulary (Home's bands are
numbered 1/2/3), not by shape — each renders a different domain: an active-sessions list, an
agents/steerers roster, a diagnostics accordion, a workflow-run inspector. `NowBand` is the most
reuse-oriented of the four (it deliberately reuses the `/sessions` row/table/group idiom rather than
inventing its own), so it is the reference point if any Band component is generalised later.

- `frontend/src/lib/components/shared/OverviewBand.svelte` — a generic strip of counts with one line
  of guidance, taking its facts as props and knowing no domain. It is what a page puts under its
  title; NowBand is a list of live sessions. Extracted from the Sessions page, where it was written
  by hand and then copied by description into other pages.
- `frontend/src/lib/components/shell/AgentsSteerersBand.svelte` — full-width band listing steerers
  and sub-agents with their own entry-rail/status treatment; not a filtered list of an existing row
  type like NowBand is.
- `frontend/src/lib/components/shell/SessionAnalysisBand.svelte` — a single collapsible accordion
  disclosure over diagnostic findings, mirroring `SessionMechanismsInventory`'s top-level disclosure;
  not a list of rows.
- `frontend/src/lib/components/shell/WorkflowRunBand.svelte` — an inline inspector for one workflow
  run (agents expand into their own sub-tree) fetched from a dedicated endpoint; a data-fetching
  detail view, not a list section.
- `frontend/src/lib/components/session-flow/FlowHeaderBand.svelte` — the Session Flow tab's static
  identity header (session id, peer, agent count, a link to the request list); a grouped
  `SectionCard` member of the flow instrument, not a Home-style page-layout section.
- `frontend/src/lib/components/session-flow/FlowFilterBand.svelte` — the flow's legend/filter toggle
  band, wrapping the shared `LegendFilter` primitive in section chrome; a grouped `SectionCard`
  member, not a list section.
- `frontend/src/lib/components/session-flow/FlowPlayerBand.svelte` — the flow's timeline instrument
  (transport, seek slider, role lanes, recorded-moments row); a grouped `SectionCard` member around
  a purpose-built playback control surface, unrelated to NowBand's row/table/group idiom.
- `frontend/src/lib/components/session-flow/FlowDetailBand.svelte` — the flow's "Selected" evidence
  summary for whatever is currently selected (actor/phase/source); a grouped `SectionCard` member,
  not a list section.

## Banner

Canonical: `frontend/src/lib/components/shared/LoadErrorBanner.svelte`

By far the most reused component in the codebase (34 importers) — the generic "I tried to load X and
it failed" banner used across routes.

- `frontend/src/lib/components/home/CapabilityBanner.svelte` — a first-run notice for a subsystem
  that is enabled but not working; not an error state, and shown even when nothing failed to load.
- `frontend/src/lib/components/shell/ConnectionBanner.svelte` — drives a connection state machine
  (connected/reconnecting/lost) sticky under the TopBar; ongoing live status, not a one-shot load
  failure.

## Card

Canonical: `frontend/src/lib/components/detail/SectionCard.svelte`

The named target of unit B4 and the highest-importer-count `*Card` component today (6). Unit B4's
review (`artifacts/threads/frontend-motion-and-consolidation/b4-card-panel-review.md`) found every
other `*Card` component genuinely shaped differently from SectionCard, except `WorkerCard`, still in
progress — see its own entry below.

- `frontend/src/lib/components/tool-calls/ToolCallCard.svelte` — keep: a two-mode dispatcher (a rich
  per-tool-kind body, or a dense timeline grid row — unit B1 of this thread merged these from two
  prior components), neither a titled section.
- `frontend/src/lib/components/shell/TurnCard.svelte` — keep for now: this file is also the direct
  target of Angle A's unit A4 restyle, due right after this review landed — reshaping its chrome here
  would collide with that in-flight work. Worth a second look once A4 is verified.
- `frontend/src/lib/components/detail/DiagnosticsCard.svelte` — keep: already composes SectionCard
  directly (its entire template is `<SectionCard title="Diagnostics">…</SectionCard>`); the file is
  domain logic (deep-link auto-expand, category help, finding list) inside SectionCard's body, not a
  duplicate of it.
- `frontend/src/lib/components/detail/RequestBodyCard.svelte` — keep: a tabbed multi-view data viewer
  (bits-ui `Tabs`), not SectionCard's single-body shape.
- `frontend/src/lib/components/detail/ResponseBodyCard.svelte` — keep: a status-driven body viewer
  with no title/collapse concept, not SectionCard's shape.
- `frontend/src/lib/components/home/DestinationCard.svelte` — keep: a documented fixed-anatomy promo
  card (caption → sentence → two bullets → stat → learn strip), a deliberately different, established
  design-system primitive, not a section.
- `frontend/src/lib/components/memory/DecisionActionCard.svelte` — keep: a shared list-row type
  across three surfaces, not a titled section.
- `frontend/src/lib/components/message-view/SystemRemindersCard.svelte` — keep: a fixed-identity
  message-stream wrapper (its own border-accent gradient), no title prop, no collapse.
- `frontend/src/lib/components/message-view/UserMessageCard.svelte` — keep: a fixed-identity message
  bubble, not a generic section.
- `frontend/src/lib/components/prompts/BlockCard.svelte` — keep: a rail-accented block viewer with a
  markdown/raw toggle, not a generic body slot.
- `frontend/src/lib/components/tool-calls/ToolResultCard.svelte` — keep: tool-result chrome paired
  with ToolCallCard, not a section.
- `frontend/src/lib/components/detail/TransformationManifestCard.svelte` — keep: a manifest data list
  embedded in a tab body, not a standalone titled section.
- `frontend/src/lib/components/filter/WorkbenchRuleCard.svelte` — keep: a draggable rule-list row
  with reorder/edit/delete controls, not SectionCard's title+body shape.
- `frontend/src/lib/components/memory/ArchivedLessonCard.svelte` — keep: one list row (lesson + why
  archived + restore control), not a titled section.
- `frontend/src/lib/components/memory/LessonRelationCard.svelte` — keep: a two-lesson comparison row
  with decide controls, not a titled section.
- `frontend/src/lib/components/settings/WorkerCard.svelte` — in progress: authorized to compose
  SectionCard with `expandable` (unit B4), mapping its on/off switch to `expandable` and moving the
  switch itself into `headerActions`; blocked on a SectionCard title capability, since its header
  shows a name AND a full description that must both stay visible even when `expandable=false` (the
  worker-off, header-only state) — SectionCard's `title` is a plain string today with nowhere for the
  second line to live.
- `frontend/src/lib/components/shell/AnalysisToolsCard.svelte` — keep: a pure frame-merge wrapper
  around two other components (one of which, `ToolUsesPanel`, now composes SectionCard itself), not a
  content section in its own right.
- `frontend/src/lib/components/shell/SystemPromptCard.svelte` — keep: several independently-
  collapsible regions in one card (billing chips, an always-visible identity quote, collapsible
  instructions/other blocks each with their own raw-JSON modal), a different, nested shape from
  SectionCard's single collapse.
- `frontend/src/lib/components/shell/files-changed-card/FilesChangedCard.svelte` — keep: a file-tree
  viewer with its own expand-all/collapse-all header controls, not a title+body section.
- `frontend/src/lib/components/settings/LaunchBackendValidationCard.svelte` — keep: same shape as
  `DiagnosticsCard` above — composes `SectionCard` directly (`collapsible={false}`, a `Validate`
  button in `headerActions`) and the file is domain logic (the live `GET /api/launches/doctor` round
  trip and its check-list render) inside SectionCard's body, not a duplicate of it.
- `frontend/src/lib/components/memory/DirectiveProposalCard.svelte` — keep: one directive proposal with its proposed text, counts, evidence link and reject action; not a collapsible titled section, so not SectionCard-shaped.
- `frontend/src/lib/components/eval/EvalAnswerCard.svelte` — keep: one answer on `/sessions/labelling/accuracy/{run}/review` (Review labels) ("This turn's label" or "The judge's answer": the word, its definition, who labelled it and why, or which model answered, its reason and what it saw), rendered as a static band inside the rail's `.edge.mech-group`, not SectionCard's collapsible title+body shape.
- `frontend/src/lib/components/judges/TestRunCard.svelte` — keep: one labelling accuracy run as a `.edge.mech-group` with a static band header (the run, named by date, configuration and model, as a link) and a fixed body (a label/value list of what was measured, a per-question score grid with Review labels links, an accuracy-bar disclosure); used on `/sessions/labelling/accuracy` and `/sessions/labelling/accuracy/{run}`. Not collapsible and its header is a link, so not SectionCard's title+collapse shape.
- `frontend/src/lib/components/improvement/LessonCandidateCard.svelte` — keep: one lesson candidate as a collapsible band inside `LessonCandidateList`'s `.mech-group` — the rule in full as the toggle, approve/reject beside it, a source line, and when expanded the full ask and correction quotes; once decided it stays in place showing Approved or Rejected. A band of a section stack, not SectionCard's standalone framed card.
- `frontend/src/lib/components/improvement/GuardWakeCard.svelte` — keep: one delivered guard nudge as a collapsible band (the question as the toggle, Right/Wrong beside it, an outcome tag, and when expanded the full note and outcome sentence), shared between the Nudges tab on What the judge did and the guard page via `GuardWakeList`; a band of a section stack, not SectionCard-shaped.

## Drawer

Canonical: `frontend/src/lib/components/shell/RequestDrawer.svelte`

The heavy-bytes, request-scoped modal (drives `shellState.drawerKind`/`drawerBandId`, tabbed body
views, `?bodyTab=` deep-linking).

- `frontend/src/lib/components/shared/HelpDrawer.svelte` — a static outbound-links reference sheet
  triggered by the TopBar's `?` button; no request-scoped state, no tabs, no deep-linking.

## Header

Canonical: `frontend/src/lib/components/detail/DetailHeader.svelte`

The page-level identity bar for the request detail page (deliberately excludes tokens/duration/cost,
which belong to `MetricsStrip`).

- `frontend/src/lib/components/detail/TurnDeltaHeader.svelte` — a card-embedded delta/full toggle
  header for one turn's diff view (flag chips + a mode toggle), not a page-level identity header.

## Indicator

Canonical: `frontend/src/lib/components/shared/StreamingIndicator.svelte`

A generic animated activity primitive (three concentric spinning arc rings) with no domain data,
used wherever "this is actively streaming" needs to render.

- `frontend/src/lib/components/shell/ValidationStatusIndicator.svelte` — by its own header comment,
  "presentational only, not yet wired into SessionTimeline/TurnCard": a pass/correction status badge
  for a turn's completion-claim check, not an animated activity indicator, and not yet integrated
  into any route.

## List

Canonical: `frontend/src/lib/components/eval/LabelHistoryList.svelte`

The first `*List` pair, both added by the judgement-context program. `LabelHistoryList` is the
canonical by genericity: its props are a plain array of label rows and two callbacks, with no
fetching of its own.

- `frontend/src/lib/components/settings/EvalAlarmList.svelte` — keep: the open alarms a page is given, each as one plain sentence (`alarmWords` in `judges/judges.ts`) with an acknowledge action; renders nothing when empty. A different row type (`EvalAlarmDto`), not a label history. Its self-loading wrapper for settings pages is `settings/PageAlarms.svelte`.
- `frontend/src/lib/components/eval/EvalTurnList.svelte` — keep: the `All turns` sheet on Review labels (`/sessions/labelling/accuracy/{run}/review`), one selectable row per accuracy-run item (session title and position loaded lazily per visible row, what the label and the judge say, what you decided); a different row type (`EvalRunItemDto`) with row selection, not a label history.
- `frontend/src/lib/components/improvement/LessonCandidateList.svelte` — keep: one `.edge.mech-group` holding a lead band, a static header per project and that project's `LessonCandidateCard` bands, with an empty line when nothing waits; a different row type (`LessonCandidate`) with its own reject-reason interaction, not a label history.
- `frontend/src/lib/components/skills/SkillLessonList.svelte` — keep: the accepted lessons a learned skill draft was written from, each in full with its kind, a link to the lesson and the prompt it was learned from; a different row type (`SkillLesson`), read-only, not a label history.
- `frontend/src/lib/components/skills/SkillRevisionList.svelte` — keep: every draft of one skill folder, newest first, each linking to its own page with the current one marked; a different row type (`SkillRevision`), not a label history.
- `frontend/src/lib/components/improvement/GuardWakeList.svelte` — keep: one `.edge.mech-group` of `GuardWakeCard` bands (optional lead band, answerable nudges, then those not judged yet under their own header), shared between the Nudges tab and the guard page; an `emptyText` prop reports an empty result; a different row type (`GuardWake`), not a label history.

## Marker

Canonical: `frontend/src/lib/components/shared/DisclosureMarker.svelte`

Documented in its own header as "the one disclosure marker for the whole app" (33 importers) — the
rotating triangle FUNK's Surfaces pattern specifies for every expand/collapse affordance.

- `frontend/src/lib/components/shell/SteerMarker.svelte` — a session-timeline row surfacing a fired
  steer at its chronological position; a timeline row component that shares the word "marker" by
  domain vocabulary (marking where a steer fired), not a rotating disclosure glyph.

## Panel

Canonical: `frontend/src/lib/components/projects/ProjectDetailPanel.svelte`

Tied on importer count (2, the two projects routes) with `ProjectSessionsPanel` and
`ProjectEmbeddingPanel`; it is the one of the three not slated to fold into SectionCard, and its
`nested` frame-drop prop is the shape the other members' `bordered`-style props echo. The previous
canonical, `message-view/RawBodyPanel.svelte`, was deleted on 2026-10-08: nothing had rendered it
since the 2026-09-08 session-page rework, and the message-view cards draw their own raw bodies.

- `frontend/src/lib/components/settings/setup/SetupFeaturesPanel.svelte` — the three feature choices
  of the Setup tab with one Apply button that changes only the switches that differ; it holds no frame of
  its own, so it is rows inside a SettingsSection, not a ProjectDetailPanel.
- `frontend/src/lib/components/settings/setup/SetupModelsPanel.svelte` — the provider and model choice
  for each background job on the Setup tab, drawn as SetupRows inside a SettingsSection; it has no frame
  of its own, so it does not extend ProjectDetailPanel.
- `frontend/src/lib/components/graph/ProjectEmbeddingPanel.svelte` — in progress: authorized to
  compose SectionCard with `bordered` driven by its existing `nested` prop (unit B4); hand-rolls
  SectionCard's own collapsible-header pattern today. Blocked on the same SectionCard title capability
  as `WorkerCard` — its header carries an icon and an explanatory Tooltip beside the title, the
  binding convention this app uses for section explanations, which a plain string `title` cannot hold.
- `frontend/src/lib/components/projects/ProjectKnowledgePanel.svelte` — keep: composes SectionCard
  directly (`collapsible`, `bordered={false}`, its embedding-coverage status in `headerActions`) rather than
  hand-rolling a header to fold later; a search box, a ranked result list and a source-reading
  drawer over `KnowledgeSearchResponse`/`KnowledgeReadResponse`, not a one-project detail view.
- `frontend/src/lib/components/projects/ProjectLaunchesPanel.svelte` — keep: composes SectionCard
  directly from the start (mike#260 step 4) rather than hand-rolling a header to fold later; a
  domain-typed list over `LaunchRow[]` (status vocabulary, row expand/kill), not a one-project
  detail view.
- `frontend/src/lib/components/sessions/TimeSpentPanel.svelte` — keep: composes SectionCard directly
  (`collapsible`, `bordered={!grouped}`, its headline numbers in `headerActions`) and is shown on
  two pages, the Sessions list and one session's mechanism group; a fetch, controls and four
  breakdowns over `TimeSpentDto`, not a one-project detail view.
- `frontend/src/lib/components/projects/ProjectSessionsPanel.svelte` — in progress, same status and
  blocker as `ProjectEmbeddingPanel.svelte` above.
- `frontend/src/lib/components/graph/SessionEmbeddingPanel.svelte` — in progress, same status and
  blocker as `ProjectEmbeddingPanel.svelte` above.
- `frontend/src/lib/components/memory/SessionDeliveryPanel.svelte` — keep: composes SectionCard
  directly (`collapsible`, `bordered={!grouped}`, `titleTooltip`, its counts in `headerActions`) and
  adds only the fetch of `GET /api/sessions/{id}/delivery` and the three section components that draw
  the profile, the delivered lessons, topics and skills, and the injection stages; one session's
  delivery record over `SessionDelivery`, not a one-project detail view.
- `frontend/src/lib/components/memory/SessionLessonsPanel.svelte` — in progress, same status and
  blocker as `ProjectEmbeddingPanel.svelte` above.
- `frontend/src/lib/components/projects/ProjectMemoryPanel.svelte` — in progress, same status and
  blocker as `ProjectEmbeddingPanel.svelte` above.
- `frontend/src/lib/components/settings/AnthropicIntegrationPanel.svelte` — keep: a compound
  form+status panel (an auth-mode radio group, a key-entry form, and a test-connection action, all
  mutating directly and asking the parent to refetch), same posture as `OpenRouterIntegrationPanel`
  below and deliberately mirrors its structure; not a simple title+body section.
- `frontend/src/lib/components/home/ScheduledJobsPanel.svelte` — keep: composes SectionCard
  directly (`titleTooltip`, the machine name in `headerActions`) and adds only the fetch, the 30-second
  refresh and the fixed row order over `ScheduledJobsResponse`; it renders nothing when the route is
  missing, which SectionCard cannot do for itself.
- `frontend/src/lib/components/settings/CapabilityPanel.svelte` — keep: a status-row list, not a
  titled section.
- `frontend/src/lib/components/settings/CodexIntegrationPanel.svelte` — keep: the Codex
  integration status panel (selected-provider badge, account and rate-limit windows), same posture
  as `AnthropicIntegrationPanel` above; not a simple title+body section.
- `frontend/src/lib/components/settings/OpenRouterIntegrationPanel.svelte` — keep: a compound
  form+status panel (a key-entry form plus independently-fetched sub-sections), not a simple
  title+body section.
- `frontend/src/lib/components/settings/SyncStatusPanel.svelte` — keep: a fixed-anatomy read-only
  status display, no collapse.
- `frontend/src/lib/components/shell/SessionControlsPanel.svelte` — done: composes SectionCard with
  `bordered={!grouped}`; the steerers/graders/rules/analyzers count line moved into `headerActions`.
- `frontend/src/lib/components/shell/SessionReconcilePanel.svelte` — keep: composes SectionCard directly;
  a single-session comparison of Claude Code's steps against captured requests, with its own load state.
- `frontend/src/lib/components/shell/SessionLaunchPanel.svelte` — keep: composes SectionCard directly
  from the start (mike#260 step 4), same posture as `ProjectLaunchesPanel.svelte` above — a
  single-record "how this session was launched" detail over one `LaunchRow`, not a project.
  Renders nothing at all (no DOM, no frame) for the
  common case where the session has no launch record, which no other Panel member does.
- `frontend/src/lib/components/shell/SessionMailboxPanel.svelte` — in progress, same status and
  blocker as `ProjectEmbeddingPanel.svelte` above.
- `frontend/src/lib/components/shell/ToolUsesPanel.svelte` — done: composes SectionCard with
  `bordered={!embedded}`; the call-count line moved into `headerActions`. Its own dead "standalone"
  header variant (this file's prior comment already said nothing mounts it with `embedded=false` in
  production) is gone — SectionCard's own frame now serves that case for free.
- `frontend/src/lib/components/filter/WorkbenchAssistantPanel.svelte` — keep: a streaming chat
  transcript (messages, proposal cards, a composer) over the prompt-assistant SSE conversation, not
  a detail view over one record; no title/collapse concept of its own — it fills its whole region of the filter workbench's side panel.
- `frontend/src/lib/components/memory/PatternCandidatesPanel.svelte` — keep: the semantic worker's pattern candidates for `/memory`, a list over `PatternCandidateDto`; the parent page owns fetching, not a one-project detail view.
- `frontend/src/lib/components/settings/GuardCalibrationPanel.svelte` — keep: the guard's calibration snapshot (thresholds in force and the rollup they came from) on `/settings/steering`; domain-typed, read-only.
- `frontend/src/lib/components/settings/GuardQuestionsPanel.svelte` — keep: the turn guard's four questions with their modes, promotion state and write controls on `/settings/steering`; a control surface, not a text display.
- `frontend/src/lib/components/settings/WorkerAccuracyPanel.svelte` — keep, unused: per-worker accuracy rates from `eval.rates`. Removed from every page on 2026-09-28 by operator decision (it measured the old outcome labels); the file stays until the operator confirms its deletion.
- `frontend/src/lib/components/judges/JudgeInUsePanel.svelte` — keep: the configuration a judge runs with now, as a label/value list inside a `.edge.mech-group` with an Edit action in its band header; domain-typed facts, not a raw-body viewer.
- `frontend/src/lib/components/judges/JudgeAgreementPanel.svelte` — keep: the latest labelling accuracy of the configuration in use (how often the judge gave the right label in its newest accuracy run), with the turns left to review and a Review labels link; domain-typed figures, not a raw-body viewer.
- `frontend/src/lib/components/improvement/GuardQuestionPanel.svelte` — keep: one turn-guard question in full (mode, fires this week, precision, holdback/effect, relapse-evidence progress, its delivered `GuardWakeList`) as an `.edge.mech-group` card on `/sessions/improvement/guard`; domain-typed figures, not a raw-body viewer.

## Popover

Canonical: `frontend/src/lib/components/settings/ModelPickerPopover.svelte`

All three are bits-ui `Popover` wrappers anchoring different content — the expected wrapping pattern
per `CONVENTIONS.md`, not shape duplication. Named canonical on importer count; the other two anchor
narrower, purpose-built content rather than being incomplete versions of it.

- `frontend/src/lib/components/filter/AddRulePopover.svelte` — a popover wrapper specifically around
  `FilterRuleForm`'s rule-edit path; the per-block "add rule" flow, and the filter workbench's edits
  list, both render `FilterRuleForm` inline without this wrapper.

## Row

Canonical: `frontend/src/lib/components/settings/SettingRow.svelte`

Highest importer count (5) of the thirteen `*Row` components. `Row` turns out to be a domain-neutral
naming convention across this frontend rather than a single shape: the thirteen members below render
thirteen genuinely different list contexts, and none is a stripped-down or extended version of another.

- `frontend/src/lib/components/settings/setup/SetupRow.svelte` — one line of the Setup tab: a label, a
  status badge, one sentence, optional controls and the terminal commands to copy; SettingRow edits one
  stored setting and has no place for a status or a command.
- `frontend/src/lib/components/proposals/ProposalRow.svelte` — one lesson placed as a rule or an
  agent statement on the Proposals page: the lesson, why it was placed there, the file it belongs in
  and the text it would add, and accept, decline and undo buttons; a decision row, not SettingRow's
  label+control+description layout.

- `frontend/src/lib/components/home/ScheduledJobRow.svelte` — one scheduled job on the home page:
  state badge, last-run and next-run times, inline detail and fix, and a disclosure of the exit code
  and full output; a job-status row, not SettingRow's label+control+description layout.
- `frontend/src/lib/components/skills/SkillFolderRow.svelte` — one learned skill folder on the
  Skills page: name, state badge, the description Claude sees, the reviewer's figures, loads since
  it was written and the path it is written to on each machine; a skill-status row, not SettingRow's
  label+control+description layout.
- `frontend/src/lib/components/sessions/SessionRow.svelte` — the canonical `/sessions`-page list-row
  (session id/status/metrics), independently reused verbatim by `NowBand` and `ProjectSessionGroup`;
  a session-summary row, not SettingRow's label+control+description layout.
- `frontend/src/lib/components/message-view/SubRow.svelte` — a leaf item inside a message subgroup
  body (optionally an expandable `<button>`); no label/control split.
- `frontend/src/lib/components/message-view/ReminderRow.svelte` — a flat, non-expandable leaf row
  for reminder categories with no children; no label/control split.
- `frontend/src/lib/components/shell/SessionEventRow.svelte` — a non-operator timeline event row
  (teammate message, recap, compaction); timeline-shaped, not a settings row.
- `frontend/src/lib/components/memory/LessonOutcomeRow.svelte` — a lesson's injection/outcome
  pairing row inside `OutcomesSection`; the data shape has no label/control split.
- `frontend/src/lib/components/prompts/PromptCatalogueRow.svelte` — a `/prompts` catalogue table row
  rendering a pre-resolved view-model; table-row shaped, not label+control.
- `frontend/src/lib/components/sessions/SearchMatchRow.svelte` — a search-result snippet row shared
  by two search lanes; a content-preview row, not label+control.
- `frontend/src/lib/components/settings/FeatureToggleRow.svelte` — an accessible switch
  (`role="switch"`) that is itself a control, not a row that hosts one like SettingRow.
- `frontend/src/lib/components/shell/InterventionRow.svelte` — an accordion-disclosure timeline row
  (collapsed chrome / expanded full content) in the steering intervention timeline; not label+control.
- `frontend/src/lib/components/filter/WorkbenchOtherPromptRow.svelte` — a filter-rule row for the
  workbench's "Edits for other prompts" / "Matches no recent prompt" groups (title, enable switch,
  inline delete confirm, jump-to-prompt link); a leaf list row over a `FilterRuleDto`, not
  SettingRow's label+control+description layout, and deliberately lighter than the sibling
  `WorkbenchRuleCard.svelte` (no before/after diff, no "reword with assistant") since these rules
  aren't anchored to the currently open prompt.
- `frontend/src/lib/components/improvement/ActionEffectRow.svelte` — keep: one band inside
  `ActionEffectTable`'s `.mech-group` — a lesson/directive/guard-question's own label (linked) and
  scope in the header, its `EffectVerdict` in the body; not SettingRow's label+control+description
  layout.

## Section

Canonical: `frontend/src/lib/components/settings/SettingsSection.svelte`

Tied at 2 importers with three other members; `SettingsSection` is the one whose own header comment
names its shape as a reusable pattern ("one settings group rendered as a single card"), used
identically across every settings page.

- `frontend/src/lib/components/memory/OutcomesSection.svelte` — a presentational lesson-outcome
  feedback panel for `/memory`; the parent page owns fetch/loading/error state, unlike
  SettingsSection's self-contained frame.
- `frontend/src/lib/components/memory/RelationsSection.svelte` — a presentational lesson-relation
  graph panel for `/memory`, same parent-owns-state split as OutcomesSection; not a settings group.
- `frontend/src/lib/components/shell/SessionSteeringSection.svelte` — a per-session intervention
  timeline using the steering-zone rose token role; a timeline, not a settings group.
- `frontend/src/lib/components/detail/IdentifiersSection.svelte` — renders only the *identifier*
  fields (who/where/which session) of a request; a strict field-scope split with its sibling
  ParametersSection, not a settings group.
- `frontend/src/lib/components/detail/ParametersSection.svelte` — renders only the *parameter*
  fields (what was asked for / how it ran); the other half of the IdentifiersSection split.
- `frontend/src/lib/components/shared/board/BoardSection.svelte` — a sub-header inside a
  `BoardColumn` (funk-update-2026-09-15.md §3.2): a dashed divider and a small machine head with
  its own collapse control, positioned at a caller-given `y`. Not a settings group or a field
  split — the instrument board's own vocabulary, sized and coloured entirely by the board's token
  set rather than SettingsSection's card frame.
- `frontend/src/lib/components/memory/DirectiveProposalsSection.svelte` — a presentational list of directive proposals for `/memory`, same parent-owns-state split as OutcomesSection; not a settings group.

## Strip

Canonical: `frontend/src/lib/components/detail/MetricsStrip.svelte`

Tied at 1 importer with the other active members; picked as canonical because it is the one other
components' comments treat as an established reference point (e.g. `DetailHeader` explicitly defers
tokens/duration/cost to it). Each `*Strip` component renders distinct, non-interchangeable stat
content — none is a subset or superset of another's cells.

- `frontend/src/lib/components/improvement/ImprovementStrip.svelte` — keep: the summary strip on What the judge did (counts waiting on you, each selecting its tab; last-7-days lessons and nudges; corrected share last week), modelled on the Sessions orientation strip; not per-request metric cells.
- `frontend/src/lib/components/home/PulseStrip.svelte` — Home's single live status line
  (ONLINE/uptime/request count/stream count + an oscilloscope trace), not per-request metric cells.
- `frontend/src/lib/components/memory/RunsStrip.svelte` — a small "did it run" facts strip
  (last session / nightly / GC pass) for `/memory`; deliberately never shows counts.
- `frontend/src/lib/components/prompts/PromptKpiStrip.svelte` — a 4-cell KPI strip specific to the
  `/prompts` list page (Registered / Shapes observed / Unregistered / Drifted).
- `frontend/src/lib/components/live/HealthStrip.svelte` — no current importers outside its own
  story; `SystemLivePanel`'s header comments ("mirrors HealthStrip") indicate it now inlines an
  equivalent panel. A candidate for removal, not consolidation — tracked here rather than silently
  left undocumented.
- `frontend/src/lib/components/llmSpend/AccountStrip.svelte` — the `/settings/integrations`
  OpenRouter-section spend supplement: days-left runway estimate, today vs. daily ceiling, and
  billed/recorded/unrecorded for the month. Deliberately does not repeat the balance/credits
  numbers `OpenRouterIntegrationPanel` already shows; its cells are the ones that panel doesn't
  cover.

## Table

Canonical: `frontend/src/lib/components/detail/HeadersTable.svelte`

Highest importer count (3); a generic masked key/value table for HTTP headers.

- `frontend/src/lib/components/settings/ModelTable.svelte` — a selectable table of steerer models
  with row-click selection semantics; HeadersTable has no selection concept.
- `frontend/src/lib/components/settings/ModelUsageTable.svelte` — a fixed, API-ordered usage-stats
  table with no selection and no value-masking logic.
- `frontend/src/lib/components/settings/ChangeLogTable.svelte` — the configuration change timeline on `/settings/history`, one row per `config.change_log` entry with its before and after values; no masking, no selection.
- `frontend/src/lib/components/llmSpend/CallerTable.svelte` — the `/settings/integrations`
  per-caller cost breakdown, sortable by any column (cost, calls, share, cost/call, avg input
  tokens, failed); HeadersTable has no sorting and no outbound row links, this table's whole
  purpose is routing a row to the steerer or worker page that caused the spend.
- `frontend/src/lib/components/improvement/ActionEffectTable.svelte` — a row-band list (like
  `TestRunCard`'s siblings on `/sessions/labelling/accuracy`), one `ActionEffectRow` per
  lesson/directive/guard question with its measured effect; prose-shaped content reads better as a
  band than as table cells, and it has neither HeadersTable's masking nor CallerTable's sorting.
