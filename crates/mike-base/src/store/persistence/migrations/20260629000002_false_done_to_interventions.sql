-- ════════════════════════════════════════════════════════════════════
-- C3 / U1: Fold false_done onto the general interventions rails.
-- EXPAND ONLY — nothing is dropped (U5 handles the drop of
-- steering.false_done_fires once the unified path is in production).
-- ════════════════════════════════════════════════════════════════════
--
-- Changes (all additive or nullability relaxations):
--   1. Make steering.interventions.enrolment_id nullable.
--      false_done is enrolment-free; all other steerers still populate it.
--   2. Add steering.interventions.missing_evidence (nullable text).
--   3. Seed a 'false_done' verdict code in steering.verdicts (identity-assigned id).
--   4. Data-migrate every steering.false_done_fires row →
--      steering.interventions (set-based INSERT…SELECT, idempotent).
--   5. Redefine steering.false_done_rate_v to read from
--      steering.interventions filtered to the false_done steerer.
--      Return shape is identical (same 6 columns) so the MCP/API
--      response structs are unchanged.
--
-- Idempotency:
--   - ALTER … DROP NOT NULL is a no-op if the column is already nullable.
--   - ADD COLUMN IF NOT EXISTS is a no-op on re-run.
--   - INSERT … ON CONFLICT DO NOTHING for the verdict seed.
--   - The data-migration INSERT guards via NOT EXISTS on interventions.uuid
--     = false_done_fires.intervention_uuid (explicitly allowed anti-join
--     per .claude/rules/sql-set-based.md — NOT EXISTS is NOT a correlated
--     scalar subquery; it runs set-based).
--   - CREATE OR REPLACE VIEW is idempotent.
--
-- Constraint notes:
--   The existing CHECK `interventions_fired_has_payload` fires only when
--   verdict_id = 1 ('fired'). Migrated rows use verdict_id = 7
--   ('false_done'), so the constraint is never triggered regardless of
--   payload_text value (all nudge_messages are non-null anyway).
--   The existing FK idx_interventions_enrolment_id is retained; it covers
--   non-null enrolment_id rows.  NULL values are not indexed by btree, so
--   false_done rows simply produce no entry there — correct.

-- ─── 1. Make enrolment_id nullable ────────────────────────────────────────
-- ALTER … DROP NOT NULL is a lightweight catalog update; it does NOT
-- rewrite the table or take an exclusive lock beyond the ALTER itself.

ALTER TABLE steering.interventions
    ALTER COLUMN enrolment_id DROP NOT NULL;

-- ─── 2. Add missing_evidence column ──────────────────────────────────────

ALTER TABLE steering.interventions
    ADD COLUMN IF NOT EXISTS missing_evidence text;

-- ─── 3. Seed the false_done verdict ──────────────────────────────────────
-- Let the identity column assign the id — the data-migration resolves the
-- verdict by CODE, so the numeric id is never referenced downstream and a
-- hardcoded value would be fragile against the live table's actual max id.
-- steering.verdicts.code carries a UNIQUE constraint, so ON CONFLICT (code)
-- DO NOTHING makes the seed re-runnable with no sequence fix-up needed.

INSERT INTO steering.verdicts (code)
    VALUES ('false_done')
ON CONFLICT (code) DO NOTHING;

-- ─── 4. Data-migrate false_done_fires → interventions ─────────────────────
-- One set-based INSERT…SELECT.  No correlated scalar subqueries.
--
-- CTEs resolve the false_done steerer (authority, mode) and the new
-- verdict once, then the main SELECT fans them across every
-- false_done_fires row that has not already been migrated (anti-join guard
-- on interventions.uuid = false_done_fires.intervention_uuid).
--
-- Column mapping:
--   interventions.steerer_id       ← false_done steerer id (kind='false_done')
--   interventions.enrolment_id     ← NULL (false_done is enrolment-free)
--   interventions.session_id       ← false_done_fires.session_id
--   interventions.verdict_id       ← id of the 'false_done' verdict (resolved by code)
--   interventions.mode_id          ← steerer.default_mode_id
--   interventions.authority_id     ← steerer.authority_id
--   interventions.trigger_request_id ← false_done_fires.trigger_request_id
--   interventions.confidence       ← false_done_fires.confidence
--   interventions.payload_text     ← false_done_fires.nudge_message
--   interventions.missing_evidence ← false_done_fires.missing_evidence
--   interventions.sent_request_id  ← false_done_fires.delivered_request_id
--   interventions.uuid             ← false_done_fires.intervention_uuid
--   interventions.created_at       ← false_done_fires.fired_at

WITH
  fd_steerer AS (
    SELECT id, authority_id, default_mode_id
    FROM   steering.steerers
    WHERE  kind = 'false_done'
    LIMIT  1
  ),
  fd_verdict AS (
    SELECT id
    FROM   steering.verdicts
    WHERE  code = 'false_done'
    LIMIT  1
  )
INSERT INTO steering.interventions
    (steerer_id, enrolment_id, session_id,
     verdict_id, mode_id, authority_id,
     trigger_request_id, confidence,
     payload_text, missing_evidence,
     sent_request_id, uuid, created_at)
SELECT
    st.id,
    NULL,
    fdf.session_id,
    v.id,
    st.default_mode_id,
    st.authority_id,
    fdf.trigger_request_id,
    fdf.confidence,
    fdf.nudge_message,
    fdf.missing_evidence,
    fdf.delivered_request_id,
    fdf.intervention_uuid,
    fdf.fired_at
FROM  steering.false_done_fires fdf
CROSS JOIN fd_steerer st
CROSS JOIN fd_verdict  v
WHERE NOT EXISTS (
    SELECT 1
    FROM   steering.interventions i
    WHERE  i.uuid = fdf.intervention_uuid
);

-- ─── 5. Redefine false_done_rate_v over steering.interventions ─────────────
-- Column shape is identical to the previous definition:
--   project_path, fires_count, corrected_count, delivered_count,
--   corrected_rate, delivered_rate
-- Return types are unchanged (bigint counts, float8 rates) so the Rust
-- FalseDoneRateRow struct and MCP/API handlers need no edits.
--
-- Source table change:
--   BEFORE: steering.false_done_fires joined via session_id → sessions → projects
--   AFTER:  steering.interventions i, filtered to the false_done steerer.
--
-- "delivered" in the new view = sent_request_id IS NOT NULL (was:
-- delivered_request_id IS NOT NULL in false_done_fires).
--
-- Two performance-driven shaping choices (verified with db-perf-check,
-- read tier, against the live capture DB — neutral vs the old view, ~4.3ms):
--
--   * The false_done steerer is selected with an IN-semi-join
--     (`i.steerer_id IN (SELECT id FROM steering.steerers WHERE kind=…)`)
--     rather than an inner JOIN into the row stream. The semi-join keeps
--     the steerers table out of the aggregated row set (it only gates the
--     interventions scan) and is set-based / safe against more than one
--     false_done steerer ever existing (a scalar `= (SELECT …)` would error
--     on >1 row). The subquery is uncorrelated — it runs once.
--
--   * The operator_turns range-join is wrapped in an `OFFSET 0` optimisation
--     fence. Driving the aggregate from interventions makes the planner
--     prefer a per-row bitmap index nested-loop over operator_turns (≈4×
--     slower at the current ~4.4k-row operator_turns size); the fence keeps
--     the single-pass hash-right-join the previous false_done_fires view
--     used (operator_turns scanned once, the range applied as a join
--     filter), restoring parity. This view is an occasionally-loaded
--     per-project dashboard aggregate — same class as session_summary_v.
--
-- DROP required because Postgres forbids changing column expressions via
-- CREATE OR REPLACE without an exact type+name match; a DROP+CREATE is
-- safer and the view has no dependent objects.

DROP VIEW IF EXISTS steering.false_done_rate_v;

CREATE VIEW steering.false_done_rate_v AS
SELECT
    p.project_path,
    COUNT(i.id)                                                               AS fires_count,
    COUNT(i.id) FILTER (WHERE o.code = 'corrected')                           AS corrected_count,
    COUNT(i.id) FILTER (WHERE i.sent_request_id IS NOT NULL)                  AS delivered_count,
    CASE
        WHEN COUNT(i.id) = 0 THEN NULL
        ELSE ROUND(
                 COUNT(i.id) FILTER (WHERE o.code = 'corrected')::numeric
                 / COUNT(i.id)::numeric,
                 4
             )::float8
    END                                                                       AS corrected_rate,
    CASE
        WHEN COUNT(i.id) = 0 THEN NULL
        ELSE ROUND(
                 COUNT(i.id) FILTER (WHERE i.sent_request_id IS NOT NULL)::numeric
                 / COUNT(i.id)::numeric,
                 4
             )::float8
    END                                                                       AS delivered_rate
FROM  steering.interventions     i
JOIN  sessions.sessions          s   ON s.id  = i.session_id
JOIN  projects.projects          p   ON p.id  = s.project_id
-- OFFSET 0 fence: keep operator_turns a single-pass hash join (see note above).
LEFT JOIN (SELECT session_id, start_request_id, end_request_id, outcome_id
             FROM memory.operator_turns OFFSET 0) ot
       ON ot.session_id        = i.session_id
      AND ot.start_request_id <= i.trigger_request_id
      AND ot.end_request_id   >= i.trigger_request_id
LEFT JOIN memory.turn_outcomes   o   ON o.id  = ot.outcome_id
WHERE i.steerer_id IN (SELECT id FROM steering.steerers WHERE kind = 'false_done')
GROUP BY p.project_path;

COMMENT ON VIEW steering.false_done_rate_v IS
    'Per-project FalseDone fire counts, corrected_rate, and delivered_rate. '
    'Reads from steering.interventions filtered to the false_done steerer. '
    'corrected_rate = fraction of fires where the nudged turn''s outcome '
    'is ''corrected''. delivered_rate = fraction of fires where the nudge '
    'was delivered (sent_request_id IS NOT NULL). NULL when fires_count = 0. '
    'steering.false_done_fires is kept for historical reference until U5 drops it.';
