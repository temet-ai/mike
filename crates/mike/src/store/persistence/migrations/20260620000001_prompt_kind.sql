-- ════════════════════════════════════════════════════════════════════
-- ROADMAP 18.1b Gap 5 — prompt_kind classification
--
-- Operator direction: "Its own schema and table, linked to session/requests,
-- with its own id, foreign keys to session and requests."
--
-- Schema layout:
--   prompt_kinds.kinds            — lookup: operator, activity-label,
--                                   hook-feedback, teammate-message
--   prompt_kinds.classifications  — one row per operator turn, keyed by
--                                   its opening request (start_request_id)
--
-- Grain: one classification row per operator turn. The request FK points
-- at memory.operator_turns.start_request_id (the user_prompt request that
-- opened the turn). The session FK is that turn's session.
--
-- DRY classification: the CASE WHEN expression below is the single source
-- of truth. The backfill below reuses it, and the capture-side Rust INSERT
-- (rust-developer's WU1 wiring) should embed the same SQL CASE rather than
-- duplicating the strings in Rust.
-- ════════════════════════════════════════════════════════════════════

CREATE SCHEMA IF NOT EXISTS prompt_kinds;

-- ── Lookup table ──────────────────────────────────────────────────────────────

CREATE TABLE prompt_kinds.kinds (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text   NOT NULL UNIQUE
);

-- Seed in deterministic id order (OVERRIDING SYSTEM VALUE mirrors turn_origins pattern).
-- First-match-wins classification priority: activity-label (1), hook-feedback (2),
-- teammate-message (3), operator (4).  operator is the catch-all / fallback.
INSERT INTO prompt_kinds.kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'activity-label'),
    (2, 'hook-feedback'),
    (3, 'teammate-message'),
    (4, 'operator');

-- ── Classification fact table ─────────────────────────────────────────────────

CREATE TABLE prompt_kinds.classifications (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- The operator turn's opening request; grain anchor.
    request_id     bigint      NOT NULL REFERENCES requests.requests(id)   ON DELETE CASCADE,
    -- Denormalised from memory.operator_turns for direct session-scoped queries
    -- without a join through operator_turns.
    session_id     bigint      NOT NULL REFERENCES sessions.sessions(id)   ON DELETE CASCADE,
    -- Classification result.
    kind_id        bigint      NOT NULL REFERENCES prompt_kinds.kinds(id),
    classified_at  timestamptz NOT NULL DEFAULT now()
);

-- Index every FK column (project convention, mandatory).
CREATE INDEX idx_pk_classifications_request_id
    ON prompt_kinds.classifications USING btree (request_id);

CREATE INDEX idx_pk_classifications_session_id
    ON prompt_kinds.classifications USING btree (session_id);

CREATE INDEX idx_pk_classifications_kind_id
    ON prompt_kinds.classifications USING btree (kind_id);

-- Composite for the primary read path: get all turns in a session filtered by kind.
CREATE INDEX idx_pk_classifications_session_kind
    ON prompt_kinds.classifications USING btree (session_id, kind_id);

-- Unique: one classification per opening request (one per operator turn by grain).
CREATE UNIQUE INDEX uq_pk_classifications_request_id
    ON prompt_kinds.classifications (request_id);

-- ── Backfill existing closed operator turns ───────────────────────────────────
--
-- Classification logic (first match wins, else 'operator'):
--   activity-label   user_prompt LIKE '%Describe your most recent action in 3-5 words%'
--   hook-feedback    user_prompt LIKE '%Stop hook feedback:%'
--   teammate-message user_prompt LIKE '%<teammate-message%'
--   operator         none of the above
--
-- Scope: closed turns only (closed_at IS NOT NULL).  Open turns are classified
-- capture-side when the turn opens.  All closed turns have a non-NULL user_prompt
-- (verified: 1892 rows, 0 null_prompt_turns before this migration).
--
-- ON CONFLICT DO NOTHING makes the backfill idempotent if re-run.

INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
SELECT
    ot.start_request_id,
    ot.session_id,
    -- Classification CASE (first match wins).  kind_id values match the seeded
    -- prompt_kinds.kinds rows above: activity-label=1, hook-feedback=2,
    -- teammate-message=3, operator=4.  Inlined to avoid per-row subquery overhead
    -- and to make the capture-side SQL reuse pattern unambiguous.
    CASE
        WHEN ot.user_prompt LIKE '%Describe your most recent action in 3-5 words%' THEN 1
        WHEN ot.user_prompt LIKE '%Stop hook feedback:%'                            THEN 2
        WHEN ot.user_prompt LIKE '%<teammate-message%'                              THEN 3
        ELSE 4
    END AS kind_id
FROM memory.operator_turns ot
WHERE ot.closed_at IS NOT NULL
ON CONFLICT (request_id) DO NOTHING;
