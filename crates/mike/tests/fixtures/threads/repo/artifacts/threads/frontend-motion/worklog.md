---
title: "Thread: frontend-motion"
summary: Two angles on the frontend, a motion-and-elevation model as code and consolidation of duplicated components.
type: Thread
status: implementing
created: 2026-09-08
thread: frontend-motion
last_session: 2026-09-08 (session 47933f70-bbb7-486f-84ec-4dd8109b0c4d) @ 662f850e
branch: main
acceptance_criteria: ## Acceptance criteria
---

## State of play
Where we are: Angle A is applied to the primitives; Angle B has merged ToolCallCard.
The next step is the session pages.
Next: (1) Finish A4 on the session pages. (2) Review the Card and Panel components.
Dependencies: judgement-context, branch-consolidation.
Branch: main
Worktree: /Users/vimes/projects/temet/temet-proxy/.claude/worktrees/motion
Files: frontend/src/lib/components/ui/motion.css, /Users/vimes/projects/temet/temet-proxy/frontend/src/lib/components/ui/motion.ts
Open questions: none.
Decisions: none.
Blockers: usage budget.

## Acceptance criteria
<acceptance-criteria>
A1. One shared motion layer defines every shared motion the app uses.
A2. No looping or pulsing animation in any changed file.
</acceptance-criteria>

## Session log
- 2026-09-08 (47933f70-bbb7-486f-84ec-4dd8109b0c4d) @ e4a99823 — thread created.
- 2026-09-08 (47933f70-bbb7-486f-84ec-4dd8109b0c4d) @ 12c837ca — Committed B1+B3 (34c71e2e), A1-3 (12c837ca).

## Decisions

- Storybook Foundations is the source of truth for motion patterns.

## Session log
- 2026-09-09 (47933f70-bbb7-486f-84ec-4dd8109b0c4d) — Resumed after the usage stop.
