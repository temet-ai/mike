-- ════════════════════════════════════════════════════════════════════
-- steering.false_done_fires + false_done_rate_v
-- ════════════════════════════════════════════════════════════════════
--
-- Persists every FalseDone steerer nudge that was sent. FalseDone is
-- config-driven and enrolment-free — it fires on any session where the
-- stage-1 gate (Overselling diagnostic OR completion keyword) and the
-- stage-2 LLM judge both clear the confidence threshold.  Reusing
-- steering.interventions would require making enrolment_id nullable on
-- an existing NOT NULL FK column; a dedicated table avoids that.
--
-- Rate linkage: false_done_rate_v JOINs false_done_fires to
-- memory.operator_turns via the request range
--   ot.start_request_id <= f.trigger_request_id <= ot.end_request_id
-- to identify which operator turn was nudged, then checks whether that
-- turn's outcome is 'corrected'.  Set-based range JOIN — no correlated
-- subqueries, no JSONB recompute job, no backfill required.
--
-- Also adds the missing session_id index on memory.operator_turns (FK
-- column with no btree index — pre-existing anti-pattern fixed here
-- opportunistically; the range JOIN in the rate view needs it).
--
-- All changes are additive (new table, new indexes, new view).
-- No existing column types, constraints, or NULL-ness are altered.
-- The old binary can run against the new schema without error.

-- ─── Fix pre-existing missing FK index on operator_turns ──────────────────
-- session_id is a NOT NULL FK to sessions.sessions(id) but had no btree
-- index; session-scoped scans fall back to a seq scan.  The rate view
-- needs this for the session_id = f.session_id equi-join.
CREATE INDEX IF NOT EXISTS idx_operator_turns_session_id
    ON memory.operator_turns USING btree (session_id, created_at DESC);

-- ─── steering.false_done_fires ────────────────────────────────────────────
-- One row per FalseDone nudge sent.  Populated by insert_false_done_fire
-- in the persistence layer, called best-effort from fleet.rs after
-- send_steerer_message succeeds.
CREATE TABLE IF NOT EXISTS steering.false_done_fires (
    id                 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- Session that was nudged. Resolved from session_uuid via inline
    -- subselect in insert_false_done_fire.
    session_id         bigint NOT NULL
                         REFERENCES sessions.sessions(id) ON DELETE CASCADE,

    -- The captured request whose assistant response the LLM judge evaluated.
    -- This is the trigger (the "done"-claiming response), not the nudge
    -- message the steerer injected.
    trigger_request_id bigint NOT NULL
                         REFERENCES requests.requests(id) ON DELETE CASCADE,

    -- LLM verdict fields populated at fire time from FalseDoneVerdict.
    confidence         double precision NOT NULL
                         CONSTRAINT fdf_confidence_range
                             CHECK (confidence >= 0.0 AND confidence <= 1.0),
    missing_evidence   text NOT NULL DEFAULT '',
    nudge_message      text NOT NULL,

    -- UUID placed in the x-proxy-steered-intervention header on the nudge
    -- send. Retained for audit; enables future back-fill of sent_request_id
    -- if the inspector ever needs to correlate the nudge response.
    intervention_uuid  uuid NOT NULL,

    fired_at           timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE steering.false_done_fires IS
    'One row per FalseDone steerer nudge sent. Enrolment-free: fires on any '
    'session where the config-driven gate and LLM judge exceed the confidence '
    'threshold. Rate linkage to memory.operator_turns uses the request range '
    '(start_request_id <= trigger_request_id <= end_request_id) in '
    'steering.false_done_rate_v — no per-fire backfill needed.';

COMMENT ON COLUMN steering.false_done_fires.trigger_request_id IS
    'requests.requests.id of the assistant response the judge evaluated — '
    'the turn that claimed completion without evidence. Not the nudge request.';
COMMENT ON COLUMN steering.false_done_fires.intervention_uuid IS
    'UUID sent in the x-proxy-steered-intervention header on the nudge request. '
    'Audit trail; not a FK (the nudge request may not always be captured).';

-- Per-FK indexes.
CREATE INDEX IF NOT EXISTS idx_false_done_fires_session_id
    ON steering.false_done_fires USING btree (session_id, fired_at DESC);

CREATE INDEX IF NOT EXISTS idx_false_done_fires_trigger_request_id
    ON steering.false_done_fires USING btree (trigger_request_id);

-- BRIN on fired_at for time-range scans (append-mostly, near-free).
CREATE INDEX IF NOT EXISTS brin_false_done_fires_fired_at
    ON steering.false_done_fires USING brin (fired_at)
    WITH (pages_per_range = '32');

-- ─── steering.false_done_rate_v ───────────────────────────────────────────
-- Per-project FalseDone fire counts and the fraction that correspond to
-- a subsequently-corrected operator turn.
--
-- "corrected" = the operator turn that contained trigger_request_id was
-- later classified as outcome='corrected' (the user had to correct work
-- the nudge correctly identified as unbacked).
--
-- Linkage: LEFT JOIN memory.operator_turns on
--   ot.session_id        = f.session_id
--   AND ot.start_request_id <= f.trigger_request_id
--   AND ot.end_request_id   >= f.trigger_request_id
-- This is a set-based range JOIN over indexed columns — no correlated
-- subqueries.  Turns are non-overlapping by design, so each
-- trigger_request_id maps to at most one operator turn.
--
-- corrected_rate is NULL when fires_count = 0 (no fires for that project).
CREATE OR REPLACE VIEW steering.false_done_rate_v AS
SELECT
    p.project_path,
    COUNT(f.id)                                                             AS fires_count,
    COUNT(f.id) FILTER (WHERE o.code = 'corrected')                         AS corrected_count,
    CASE
        WHEN COUNT(f.id) = 0 THEN NULL
        ELSE ROUND(
                 COUNT(f.id) FILTER (WHERE o.code = 'corrected')::numeric
                 / COUNT(f.id)::numeric,
                 4
             )::float8
    END                                                                     AS corrected_rate
FROM  steering.false_done_fires   f
JOIN  sessions.sessions           s   ON s.id  = f.session_id
JOIN  projects.projects           p   ON p.id  = s.project_id
LEFT JOIN memory.operator_turns   ot  ON ot.session_id        = f.session_id
                                     AND ot.start_request_id <= f.trigger_request_id
                                     AND ot.end_request_id   >= f.trigger_request_id
LEFT JOIN memory.turn_outcomes    o   ON o.id  = ot.outcome_id
GROUP BY p.project_path;

COMMENT ON VIEW steering.false_done_rate_v IS
    'Per-project FalseDone fire counts and corrected_rate. '
    'corrected_rate = fraction of fires where the nudged turn''s outcome '
    'is ''corrected''. NULL when fires_count = 0.';
