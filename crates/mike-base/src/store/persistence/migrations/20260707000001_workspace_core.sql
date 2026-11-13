-- ROADMAP Phase 22 / Phase 1 — "workspace" schema core.
--
-- Registration + liveness lease (session_registrations) and the durable
-- who-did-what ledger (event_kinds + coordination_events). This is the
-- entire Phase 1 slice per artifacts/2026-07-04/session-coordination/
-- implementation-plan.md §2 Phase 1, as amended by scope-rebalance.md.
--
-- Deliberately OUT of this migration (later phases): area_claims, locks,
-- project_docs, agent_queries, NOTIFY triggers, claude_proxy.* read views,
-- the workspace_coordination feature toggle, and any events_retention_days
-- settings column. Phase 1's roster is assembled handler-side over the base
-- tables here (D16) — no view exists yet for hooks/REST to read.
--
-- Deviations from design.md's schema draft, each justified below at the
-- point of use: (1) `intent` column dropped (D12 — session_goals is the one
-- place "what is this session doing" lives); (2) event_kinds seeded with
-- only registered/ended/expired/note — claimed/released/conflict/lock_* and
-- shipped arrive with their own phases per plan §2, and `shipped` is cut
-- outright (D13); (3) actor_label documented as the full session UUID, not
-- a prefix (D14 — the plan's own correction over design.md's stale "prefix"
-- comment); (4) event_kinds uses `code`/`bigint identity` (not the design
-- draft's `key`/`smallint`) to match every existing vocab table in this repo
-- (rules.kinds, steering.authorities/modes/verdicts, session_judges.outcomes)
-- — repo convention over the one-off draft spelling; (5) FK-to-vocab columns
-- declare `ON DELETE RESTRICT` explicitly, matching the most recent
-- convention (steering_schema.sql) rather than baseline's implicit default.

CREATE SCHEMA IF NOT EXISTS workspace;

-- ─── session_registrations — one row per live/recent hook registration ────
--
-- PK = FK on session_id (session_goals pattern): a registration is 1:1 with
-- a session and meaningless without it, so it CASCADEs. project_id CASCADEs
-- too, matching sessions.sessions.project_id's own cascade behavior.
--
-- No `intent` column (D12): the register handler upserts sessions.session_goals
-- instead when the hook supplies one: two competing "what is this session
-- doing" declarations was the rejected alternative.
--
-- lease_expires_at is the liveness lease read by every roster query
-- (`now() < lease_expires_at`); Phase 1 has no worker, so a stale row that
-- is never explicitly ended simply reads as expired — it costs nothing
-- because nothing (no claim, no lock) depends on it yet.
CREATE TABLE workspace.session_registrations (
    session_id       bigint      PRIMARY KEY
                        REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    project_id       bigint      NOT NULL
                        REFERENCES projects.projects(id) ON DELETE CASCADE,
    cwd              text        NOT NULL,
    worktree         text,
    registered_via   text        NOT NULL DEFAULT 'hook'
                        CHECK (registered_via IN ('hook', 'mcp', 'rest', 'passive', 'operator')),
    registered_at    timestamptz NOT NULL DEFAULT now(),
    lease_expires_at timestamptz NOT NULL,
    released_at      timestamptz
);

COMMENT ON COLUMN workspace.session_registrations.cwd IS
    'Raw, verbatim cwd string from the hook payload — a client-side signal, not validated against project_id.';
COMMENT ON COLUMN workspace.session_registrations.worktree IS
    'The /.claude/worktrees/<name> segment when the hook fires inside a worktree checkout; NULL for a main-checkout session.';
COMMENT ON COLUMN workspace.session_registrations.registered_via IS
    'Passive back-fill (Phase 2, D23a) covers every project seen in traffic (scope-rebalance R5/Q5), so passive is already a first-class value here, not a later ALTER.';
COMMENT ON COLUMN workspace.session_registrations.released_at IS
    'Explicit deregistration (register/end route). NULL is the common case — most registrations end by lease expiry, never by an explicit call.';

-- The roster fetch filters on project_id alone (liveness is a CASE over
-- lease_expires_at, not a predicate); lease_expires_at rides along so the
-- lease-expiry sweep can find a project's expired leases without a second
-- index. session_id needs no extra index beyond its PK.
CREATE INDEX idx_session_registrations_project_lease
    ON workspace.session_registrations (project_id, lease_expires_at);

-- ─── event_kinds — closed-set vocabulary for the coordination ledger ──────
--
-- bigint identity + `code` matches every existing vocab table in this repo
-- (rules.kinds, steering.authorities/modes/verdicts, session_judges.outcomes)
-- rather than design.md's one-off `key text` / `smallint` spelling.
CREATE TABLE workspace.event_kinds (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text   NOT NULL UNIQUE
);

-- Phase 1 seeds only the four kinds this phase can emit. claimed/released/
-- conflict arrive with Phase 3, lock_acquired/lock_released/lock_expired/
-- lock_stolen/lock_violation with Phase 4. `shipped` is cut outright (D13
-- — ship/correction outcomes live in memory.operator_turns, not here).
INSERT INTO workspace.event_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'registered'),
    (2, 'ended'),
    (3, 'expired'),
    (4, 'note')
ON CONFLICT (id) DO NOTHING;
SELECT setval('workspace.event_kinds_id_seq', 4);

-- ─── coordination_events — append-only "who did what" ledger ──────────────
--
-- Durable rows (D2): session_id FKs ON DELETE SET NULL rather than CASCADE,
-- because the ledger must survive the session TTL sweep. actor_label is
-- denormalized at write time to carry attribution across that sweep.
--
-- actor_label stores the FULL session UUID plus title, not a truncated
-- prefix (D14): after the sweep it is the only surviving attribution, and
-- prefixing would be a permanent, un-recoverable truncation for zero space
-- saved — the exact pattern inspection-fidelity.md bans.
--
-- `detail` is genuinely schemaless: its shape varies per event kind (a
-- conflict event's peer/prefix pair looks nothing like a lock event's
-- token/expiry pair) and nothing in the roster/board queries filters or
-- sorts on its contents — every filtered/sorted fact (project_id, session_id,
-- kind_id, created_at) is already its own typed column beside this JSONB.
CREATE TABLE workspace.coordination_events (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    project_id  bigint      NOT NULL
                    REFERENCES projects.projects(id) ON DELETE CASCADE,
    session_id  bigint
                    REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    actor_label text        NOT NULL,
    kind_id     bigint      NOT NULL
                    REFERENCES workspace.event_kinds(id) ON DELETE RESTRICT,
    subject     text        NOT NULL,
    detail      jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at  timestamptz NOT NULL DEFAULT now()
);

COMMENT ON COLUMN workspace.coordination_events.actor_label IS
    'Full session UUID + title denormalized at write time (D14) — never a prefix. The only surviving attribution once session_id goes NULL after the TTL sweep.';
COMMENT ON COLUMN workspace.coordination_events.subject IS
    'Free-text ledger subject: session UUID for registered/ended/expired, or the reconciliation detail for note (D9). Lock name and claim prefix become subjects in later phases.';

-- Board/roster read: events for one project, most recent first.
CREATE INDEX idx_coordination_events_project ON workspace.coordination_events (project_id, id DESC);
-- FK indexes (every FK gets one, no exceptions).
CREATE INDEX idx_coordination_events_session ON workspace.coordination_events (session_id);
CREATE INDEX idx_coordination_events_kind ON workspace.coordination_events (kind_id);
-- Append-mostly time series — BRIN per repo convention for this workload shape.
CREATE INDEX brin_coordination_events_created ON workspace.coordination_events USING brin (created_at);
