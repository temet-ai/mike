-- Restore the sent → delivered confirm-protocol for steering interventions.
--
-- sent_request_id    = attempt: stamped when the reminder is injected into the
--                      outgoing request body (before upstream is called).
-- delivered_request_id = confirmed: stamped after the upstream returns 2xx,
--                        proving Anthropic accepted the request carrying the nudge.
-- processed_at       = timestamp of confirmed delivery (set alongside
--                      delivered_request_id).
--
-- This column was present as `false_done_fires.delivered_request_id` (added in
-- migration 20260628000001) and lost during the false_done→interventions
-- unification (20260629000002) which mapped it to `sent_request_id`, collapsing
-- the two-stage protocol into one column. This migration restores the distinction.
--
-- After applying: run `cargo sqlx prepare --workspace --database-url $PROXY_DATABASE_URL`
-- to refresh the .sqlx offline cache for the new query in backfill_delivered_request_id
-- and the updated fetch_false_done_counts_for_session SQL.

-- ─── 1. Add the column ────────────────────────────────────────────────────────

ALTER TABLE steering.interventions
    ADD COLUMN IF NOT EXISTS delivered_request_id bigint
        REFERENCES requests.requests(id) ON DELETE SET NULL;

-- Partial index — delivered rows only, keeps the index lean.
CREATE INDEX IF NOT EXISTS idx_interventions_delivered_request_id
    ON steering.interventions (delivered_request_id)
    WHERE delivered_request_id IS NOT NULL;

-- ─── 2. Update false_done_rate_v to count confirmed deliveries ────────────────
-- Previously counted sent_request_id IS NOT NULL (attempts).
-- Now counts delivered_request_id IS NOT NULL (confirmations).
-- Shape is identical — same 6 columns — so downstream DTOs are unchanged.

CREATE OR REPLACE VIEW steering.false_done_rate_v AS
SELECT
    p.project_path,
    count(i.id)                                                                         AS fires_count,
    count(i.id) FILTER (WHERE o.code = 'corrected')                                     AS corrected_count,
    count(i.id) FILTER (WHERE i.delivered_request_id IS NOT NULL)                       AS delivered_count,
    CASE WHEN count(i.id) = 0 THEN NULL::double precision
         ELSE round(
             count(i.id) FILTER (WHERE o.code = 'corrected')::numeric
             / count(i.id)::numeric, 4)::double precision
    END AS corrected_rate,
    CASE WHEN count(i.id) = 0 THEN NULL::double precision
         ELSE round(
             count(i.id) FILTER (WHERE i.delivered_request_id IS NOT NULL)::numeric
             / count(i.id)::numeric, 4)::double precision
    END AS delivered_rate
FROM steering.interventions i
JOIN sessions.sessions s ON s.id = i.session_id
JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN (
    SELECT operator_turns.session_id,
           operator_turns.start_request_id,
           operator_turns.end_request_id,
           operator_turns.outcome_id
      FROM memory.operator_turns
    OFFSET 0
) ot ON ot.session_id = i.session_id
       AND ot.start_request_id <= i.trigger_request_id
       AND ot.end_request_id >= i.trigger_request_id
LEFT JOIN memory.turn_outcomes o ON o.id = ot.outcome_id
WHERE i.steerer_id IN (
    SELECT steerers.id FROM steering.steerers WHERE steerers.kind = 'false_done'
)
GROUP BY p.project_path;
