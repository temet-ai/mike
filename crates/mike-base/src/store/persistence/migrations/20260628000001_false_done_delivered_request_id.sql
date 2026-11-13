-- ════════════════════════════════════════════════════════════════════
-- steering.false_done_fires: delivered_request_id column
-- + drop the dead 'steered' turn_origin seed row
-- ════════════════════════════════════════════════════════════════════
--
-- Part 1: Add delivered_request_id to steering.false_done_fires.
--
-- This column records which requests.requests row carried the FalseDone
-- nudge as an injected <system-reminder>. Before this column, the nudge
-- was sent via an out-of-band POST (send_steerer_message) that was never
-- captured, so no request_id was ever stamped. The new async injection
-- path (plan section D) enqueues the nudge into the next eligible
-- Primary/PrimaryFirst request, then stamps this column via
-- backfill_false_done_delivered_request_id after the request is minted.
--
-- NULL = nudge not yet delivered (queued but not yet injected into a
-- captured request, or fires that predate this migration). The delivery
-- rate is exposed via false_done_rate_v (extended below).
--
-- Part 2: Drop the 'steered' turn_origin seed row (id=10).
-- The out-of-band send never wrote a captured request, so no
-- requests.requests row ever carried turn_origin_id = 10. This is
-- verified before applying: COUNT = 0 at migration time.
-- With delivery via the reminder queue, turn_origin is the Primary/
-- PrimaryFirst origin of the carrying request — there is no separate
-- 'steered' origin.
--
-- All changes are additive or clean-up of a dead row. No existing data
-- is altered. The old binary can run against this schema: the new column
-- defaults NULL, and the dead seed row was never referenced.

-- ─── Part 1: delivered_request_id column ─────────────────────────────────────

ALTER TABLE steering.false_done_fires
    ADD COLUMN IF NOT EXISTS delivered_request_id bigint
        REFERENCES requests.requests(id) ON DELETE SET NULL;

COMMENT ON COLUMN steering.false_done_fires.delivered_request_id IS
    'requests.requests.id of the request that carried this nudge as an '
    'injected <system-reminder>. NULL until the nudge is delivered to an '
    'eligible Primary/PrimaryFirst call. Stamped by '
    'backfill_false_done_delivered_request_id after the request is minted.';

-- FK index: every FK column must have a btree index.
CREATE INDEX IF NOT EXISTS idx_false_done_fires_delivered_request_id
    ON steering.false_done_fires USING btree (delivered_request_id)
    WHERE delivered_request_id IS NOT NULL;

-- ─── Part 2: drop the dead 'steered' turn_origin row ────────────────────────
-- Verified at migration authoring time: 0 requests.requests rows
-- reference turn_origin_id=10 ('steered'). Safe to delete.
DELETE FROM requests.turn_origins WHERE code = 'steered';

-- ─── Extend false_done_rate_v to include delivery counts ────────────────────
--
-- Adds delivered_count and delivered_rate to the existing
-- per-project fire/corrected summary.
--
-- delivered_count = fires where delivered_request_id IS NOT NULL
--   (the nudge reached an eligible request).
-- delivered_rate  = delivered_count / fires_count (NULL when no fires).
--
-- DROP is required because Postgres forbids renaming view columns via
-- CREATE OR REPLACE VIEW — the new columns shift ordinal positions.
-- No dependent objects exist on this view.
DROP VIEW IF EXISTS steering.false_done_rate_v;

CREATE VIEW steering.false_done_rate_v AS
SELECT
    p.project_path,
    COUNT(f.id)                                                                 AS fires_count,
    COUNT(f.id) FILTER (WHERE o.code = 'corrected')                             AS corrected_count,
    COUNT(f.id) FILTER (WHERE f.delivered_request_id IS NOT NULL)               AS delivered_count,
    CASE
        WHEN COUNT(f.id) = 0 THEN NULL
        ELSE ROUND(
                 COUNT(f.id) FILTER (WHERE o.code = 'corrected')::numeric
                 / COUNT(f.id)::numeric,
                 4
             )::float8
    END                                                                         AS corrected_rate,
    CASE
        WHEN COUNT(f.id) = 0 THEN NULL
        ELSE ROUND(
                 COUNT(f.id) FILTER (WHERE f.delivered_request_id IS NOT NULL)::numeric
                 / COUNT(f.id)::numeric,
                 4
             )::float8
    END                                                                         AS delivered_rate
FROM  steering.false_done_fires   f
JOIN  sessions.sessions           s   ON s.id  = f.session_id
JOIN  projects.projects           p   ON p.id  = s.project_id
LEFT JOIN memory.operator_turns   ot  ON ot.session_id        = f.session_id
                                     AND ot.start_request_id <= f.trigger_request_id
                                     AND ot.end_request_id   >= f.trigger_request_id
LEFT JOIN memory.turn_outcomes    o   ON o.id  = ot.outcome_id
GROUP BY p.project_path;

COMMENT ON VIEW steering.false_done_rate_v IS
    'Per-project FalseDone fire counts, corrected_rate, and delivered_rate. '
    'corrected_rate = fraction of fires where the nudged turn''s outcome '
    'is ''corrected''. delivered_rate = fraction of fires where the nudge '
    'was injected into a captured request. NULL when fires_count = 0.';
