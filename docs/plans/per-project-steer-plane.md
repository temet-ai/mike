---
title: Per-project steer plane
status: active
created: 2026-07-20
updated: 2026-07-20
summary: Scope steerers to a project so each repo's operator authors its own steering rules; ship the agent-lane guard as the first project steerer.
---

# Per-project steer plane

## Goal

Today every `steering.steerers` row is global — it fires for sessions in every
project. Make steerers **project-scopable** so a repo's operator can author
steering rules that only touch that repo's sessions. The motivating first rule:
an **agent-lane guard** that nudges when code is edited by the wrong actor (the
main orchestrator editing `.rs` directly instead of dispatching to
`rust-developer`; a `.sql` migration touched by a non-`db-dev` lane; etc.).

## Non-goals

- No new parallel "steer plane" subsystem or table — this is project *scoping*
  on the existing steerer model, CRUD, fleet, and UI.
- No change to how reminders are injected (still drain on Primary/PrimaryFirst
  turns, `handler.rs:653`). The wrong-subagent nudge surfaces to the orchestrator
  on its next turn, not to the subagent mid-flight — an accepted limitation.
- No MCP steerer-CRUD tools (steerer CRUD is REST-only today; keep it that way —
  MCP is demand-driven per CLAUDE.md).

## Design decisions (locked)

1. **Scope column.** Add `steering.steerers.project_id bigint NULL REFERENCES
   projects.projects(id) ON DELETE CASCADE`. **NULL = global** (every existing
   row stays global — nothing regresses). Non-NULL = fires only for sessions in
   that project. Index it partial: `WHERE project_id IS NOT NULL`.
2. **API speaks `project_path` (string); DB speaks `project_id` (bigint).** The
   SPA and `GET /api/projects` key projects by `project_path`; the REST handler
   resolves path → id (must already exist; 400 on unknown path; absent/null =
   global). `SteererDto` returns both `project_id` and joined `project_path` for
   display.
3. **Enforcement is at enrolment write-time, not in the fleet hot path.**
   - Auto-enrol query gains `AND (st.project_id IS NULL OR st.project_id =
     s.project_id)` (`steering.rs` ~478).
   - Manual `enrol_session` validates the steerer's `project_id` (when set)
     matches the session's project, rejecting a cross-project enrol (409/400).
   - Consequence: `list_active_enrolments_for_session` is already
     project-correct, so **the fleet evaluation path needs no project lookup**.
   - `force_fire` (dev-only firehose) intentionally ignores project scope, as it
     already ignores enrolment, debounce, and confidence — documented, not fixed.
4. **Scope is patchable.** `SteererPatch` may move a steerer between
   global and a project (and between projects).
5. **`merge_projects` must re-point `steering.steerers.project_id`.** It already
   re-points every project FK; this is a new one — add it to that set (db-dev).
6. **Actor signal into the judge (the piece that makes agent-lane steerers
   possible).** The in-memory request already carries `turn_origin`
   (`"primary"`/`"subagent"`) and `subagent_persona` (e.g. `"Frontend
   Development Expert"`) — `models.rs:826-838` — but the fleet never surfaces
   them. Thread both into `EvalInputs` and emit one line in
   `build_judge_user_message`:
   - `Acting agent: main orchestrator (not a specialist subagent)` for a Primary
     turn, or
   - `Acting agent: subagent · <persona>` for a subagent turn.
   `agent_instance_id` is backfilled after the turn, so it is NOT usable live —
   use `turn_origin` + `subagent_persona`, which are populated at capture.
7. **The agent-lane guard ships as a project steerer authored through the plane**
   (created live via REST after deploy), NOT as a migration seed — a scoped seed
   would need the project row to exist at migration time on a fresh DB. This also
   doubles as the live acceptance evidence.

## Unit breakdown (file-level targets)

### Unit A — db-dev (schema + SQL + perf)
- New migration `…_steerer_project_scope.sql` (expand-only, idempotent):
  - `ALTER TABLE steering.steerers ADD COLUMN IF NOT EXISTS project_id bigint
    REFERENCES projects.projects(id) ON DELETE CASCADE;`
  - Partial index `idx_steerers_project_id … WHERE project_id IS NOT NULL`.
  - Auto-enrol query (`enrol_auto_steerers`, `steering.rs` ~466-489): add the
    `project_id IS NULL OR = s.project_id` filter.
  - `merge_projects` re-point: add `steering.steerers.project_id` to the FK
    re-point set.
- SQL threading in `store/persistence/steering.rs`: `insert_steerer`,
  `insert_steerer_with_bootstrapped_agent`, `update_steerer` accept
  `project_id: Option<i64>`; `list_steerers` / `fetch_steerer_by_id` SELECT
  `project_id` and LEFT JOIN `projects.projects` for `project_path`; optional
  `project_path` filter arg on the list query.
- A `resolve_project_id_by_path(&str) -> Option<i64>` helper if none exists.
- `SteererRow` gains `project_id: Option<i64>` + `project_path: Option<String>`.
- Regenerate `.sqlx` (`cargo sqlx prepare --workspace`), commit it.
- `db-perf-check` on the changed auto-enrol query and any changed list query;
  paste `Perf:` block(s) into the commit (G6 gate).

### Unit B — rust-developer (domain + service + REST + fleet actor signal)
- `steerer/registry.rs`: `Steerer` gains `project_id: Option<i64>` (+ carry
  `project_path` if the DTO needs it from the registry; else resolve in handler).
- `steerer/service.rs`: `create`/`update` thread `project_id` (resolved from
  `project_path` — resolve in the handler or service; pick one seam).
- `api/handlers.rs`: `SteererCreate` + `SteererPatch` gain `project_path`
  (`Option<String>`; patch must be able to clear to global); `SteererDto` gains
  `project_id` + `project_path`. `create_steerer`/`patch_steerer` resolve path →
  id (400 on unknown). `list_steerers` honors optional `?project=<path>`.
  `enrol_session` rejects a cross-project manual enrol.
- `steerer/fleet.rs`: read `turn_origin` + `subagent_persona` from the store
  request in `evaluate_and_maybe_fire` (alongside `tool_calls`); add both to
  `EvalInputs`; emit the `Acting agent:` line in `build_judge_user_message`. No
  project lookup here (enforcement is at enrolment).
- Unit tests: `build_judge_user_message` includes the actor line (primary vs
  subagent); scope round-trips through create→list; cross-project manual enrol
  rejected.

### Unit C — frontend-developer (steer-plane UI)
- `lib/types.ts`: `SteererCreate`/`SteererPatch` gain `project_path?: string`;
  `SteererDto` gains `project_id: number | null` + `project_path: string | null`.
- `lib/api/steering.ts`: pass `project_path` through create/patch.
- `routes/steering/[id]/+page.svelte`: a **Project scope** control in the Basic
  information fieldset — a select populated by `getFleetProjects()` with a
  "Global (all projects)" option; default new steerers to Global. Include it in
  the create + patch payloads.
- Roster (`lib/components/live/SteererFleet.svelte` and/or `/settings/steering`):
  a **Scope** column showing `project_path` or "Global".
- Storybook: update the single representative story in `steerer-editor.stories`
  and `[id]/page.stories` to exercise the scope control (no new per-variant
  stories — single-representative rule).
- Verify via `scripts/sb-test` (typecheck + unit + storybook).

## Acceptance criteria

<acceptance-criteria>
1. A steerer can be created scoped to a project (`project_path` set) and one left
   global (`project_path` absent); `GET /api/steerers` returns `project_id` +
   `project_path` for each, NULL/absent for global. [live: REST create + list]
2. Existing steerers are unchanged — all read back as global (`project_id` NULL);
   no existing steerer changes behavior. [migration is expand-only; live list]
3. A project-scoped, `auto_enrol=true` steerer auto-enrols ONLY sessions whose
   project matches; a session in a different project is NOT enrolled in it, while
   global auto-enrol steerers still enrol everywhere. [live: two sessions in two
   projects; inspect `steering.enrolments`]
4. A manual `POST …/enrolments` of a session into a project-scoped steerer whose
   project differs is rejected with a 4xx. [live: REST]
5. The steerer judge's user message contains an `Acting agent:` line —
   "main orchestrator …" for a Primary turn and "subagent · <persona>" for a
   subagent turn. [unit test on `build_judge_user_message` + one live capture]
6. The **agent-lane guard**, authored as a temet-proxy-scoped steerer through the
   plane, FIRES when a Primary turn edits `.rs`/`.svelte`/`.sql` directly (no
   matching specialist dispatch) and does NOT fire when the matching specialist
   subagent performs the edit. [live: real editing session through the proxy]
7. The steering create/edit UI has a Project-scope control and the roster shows a
   Scope column; both render and round-trip a scoped + a global steerer.
   [live UI via Chrome MCP or sb-test story]
8. Gates green: `cargo fmt`, `cargo clippy --all-targets -D warnings`,
   `make check`, `scripts/sb-test`, `scripts/fidelity-check` CLEAN, G6 perf
   evidence in the db commit.
</acceptance-criteria>

## Verification plan

- Backend unit/integration: `cargo nextest run -p mike --test <steering binaries>`
  for scope round-trip + cross-project enrol rejection + judge actor line.
- Live: start the stack (`scripts/start-proxy.sh`), create one global + one
  temet-proxy-scoped steerer via REST, drive a session that edits `.rs` directly
  and one that dispatches `rust-developer`, and observe the agent-lane guard fire
  on the former only (inspect `/api/requests` + steerer interventions).
- UI: `scripts/sb-test` for the story; Chrome MCP load of `/steering/<id>` to
  confirm the scope control round-trips.
