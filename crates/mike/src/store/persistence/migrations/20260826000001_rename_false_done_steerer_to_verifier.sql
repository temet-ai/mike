-- Issue #159: rename the `false_done` steerer to `verifier` across the
-- data this migration owns — "false done" named the symptom the old
-- stop-weasel-gate hook chased; `verifier` names the job the steerer
-- fleet's completion-claim judge actually does. The Rust/TS/doc renames
-- ride in the same commit as this migration; this file is the live-data
-- half.
--
-- Idempotent: every UPDATE is keyed on the OLD value, so a re-run after
-- the first application is a no-op (no row still carries the old value to
-- match). The CHECK constraint swap uses DROP/ADD, which Postgres accepts
-- unconditionally on re-run since `IF EXISTS` guards the drop. The view
-- swap uses `CREATE OR REPLACE VIEW`, not a bare `CREATE VIEW` — the first
-- application's `DROP VIEW IF EXISTS steering.false_done_rate_v` no-ops on
-- re-run (the view was already renamed away), but a bare `CREATE VIEW`
-- would then fail with "relation already exists" against the view this
-- same migration created last time. Caught by re-running this file twice
-- against a real database before trusting the idempotency claim, not by
-- reading the SQL and assuming it.

-- ─── 1. steering.steerers: rename the row + its kind CHECK value ─────────
-- Order matters: widen the constraint to allow 'verifier' before any row
-- is set to it, then narrow again once no row still carries 'false_done'.

ALTER TABLE steering.steerers
    DROP CONSTRAINT IF EXISTS steerers_kind_check;

ALTER TABLE steering.steerers
    ADD CONSTRAINT steerers_kind_check
        CHECK (kind IN ('refocus', 'memory_keeper', 'verifier', 'false_done'));

UPDATE steering.steerers
   SET name = 'verifier',
       display_name = 'Verifier',
       kind = 'verifier',
       updated_at = NOW()
 WHERE name = 'false-done';

ALTER TABLE steering.steerers
    DROP CONSTRAINT IF EXISTS steerers_kind_check;

ALTER TABLE steering.steerers
    ADD CONSTRAINT steerers_kind_check
        CHECK (kind IN ('refocus', 'memory_keeper', 'verifier'));

-- ─── 2. agents.agents: rename the backing agent row ──────────────────────
-- NOT 'verifier' — agents.agents already has an unrelated row by that exact
-- name (id 1, the dispatchable "verifier" agent persona used for run-ahead
-- verification work throughout this project, e.g. Agent(subagent_type:
-- "verifier")), and agents.agents.name carries a case-insensitive UNIQUE
-- index (uq_agents_lower_name). A bare 'verifier' here collided with it and
-- was caught by scripts/deploy-proxy.sh's smoke test against the real
-- database before any cutover, not discovered by hand. This row is the
-- steerer's own backing identity (referenced only by steering.steerers.
-- agent_id, a plain foreign key — nothing looks it up by name), so
-- 'verifier-steerer' is unambiguous with the real verifier persona and
-- still clearly names what changed, matching this project's existing
-- hyphenated persona convention (rust-developer, fleet-coordinator, …).

UPDATE agents.agents
   SET name = 'verifier-steerer',
       description = 'Verifier steerer — detects unbacked completion claims and injects a [verification] nudge',
       updated_at = NOW()
 WHERE name = 'false-done';

-- ─── 3. steering.verdicts: rename the verdict code ───────────────────────
-- `code` carries a UNIQUE constraint, no CHECK enum — a plain rename.

UPDATE steering.verdicts
   SET code = 'verifier'
 WHERE code = 'false_done';

-- ─── 4. steering.false_done_rate_v → steering.verifier_rate_v ────────────
-- DROP + CREATE (not ALTER VIEW ... RENAME) because the view body's own
-- WHERE clause hardcodes the kind literal, which must move from
-- 'false_done' to 'verifier' along with the object's name. Body copied
-- from the view's CURRENT live definition — migration 20260805200001's
-- judge-failure-reason redefinition, not the older 20260629000002 one —
-- so the judge-failed-fires exclusion and excluded_judge_failed_count
-- column carry forward unchanged; only the kind literal and identifier
-- move. Column shape is otherwise identical, so `VerifierRateRow`/the MCP
-- and API handlers need no edits beyond the identifier rename already
-- applied to the Rust source.

DROP VIEW IF EXISTS steering.false_done_rate_v;

CREATE OR REPLACE VIEW steering.verifier_rate_v AS
SELECT
    p.project_path,
    count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)              AS fires_count,
    count(i.id) FILTER (WHERE o.code = 'corrected')                        AS corrected_count,
    count(i.id) FILTER (
        WHERE i.delivered_request_id IS NOT NULL
          AND ot.judge_failure_reason IS NULL
    )                                                                       AS delivered_count,
    CASE WHEN count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL) = 0
         THEN NULL::double precision
         ELSE round(
             count(i.id) FILTER (WHERE o.code = 'corrected')::numeric
             / count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)::numeric, 4)::double precision
    END AS corrected_rate,
    CASE WHEN count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL) = 0
         THEN NULL::double precision
         ELSE round(
             count(i.id) FILTER (
                 WHERE i.delivered_request_id IS NOT NULL
                   AND ot.judge_failure_reason IS NULL
             )::numeric
             / count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)::numeric, 4)::double precision
    END AS delivered_rate,
    count(i.id) FILTER (WHERE ot.judge_failure_reason IS NOT NULL)          AS excluded_judge_failed_count
FROM steering.interventions i
JOIN sessions.sessions s ON s.id = i.session_id
JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN (
    SELECT operator_turns.session_id,
           operator_turns.start_request_id,
           operator_turns.end_request_id,
           operator_turns.outcome_id,
           operator_turns.judge_failure_reason
      FROM memory.operator_turns
    OFFSET 0
) ot ON ot.session_id = i.session_id
       AND ot.start_request_id <= i.trigger_request_id
       AND ot.end_request_id >= i.trigger_request_id
LEFT JOIN memory.turn_outcomes o ON o.id = ot.outcome_id
WHERE i.steerer_id IN (
    SELECT steerers.id FROM steering.steerers WHERE steerers.kind = 'verifier'
)
GROUP BY p.project_path;

COMMENT ON VIEW steering.verifier_rate_v IS
    'Per-project Verifier fire counts, corrected_rate, and delivered_rate, '
    'excluding fires whose turn''s judge call failed (see excluded_judge_failed_count). '
    'Reads from steering.interventions filtered to the verifier steerer. '
    'corrected_rate = fraction of counted fires where the nudged turn''s outcome '
    'is ''corrected''. delivered_rate = fraction of counted fires where the nudge '
    'was delivered (delivered_request_id IS NOT NULL). NULL when fires_count = 0.';
