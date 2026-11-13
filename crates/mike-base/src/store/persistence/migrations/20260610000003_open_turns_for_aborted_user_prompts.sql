-- Repair operator turns for aborted user-prompt requests
-- (thread: turn-prompt-extraction, reported 2026-06-10 — request 2990 in
-- session b72728f8 rendered as a bare band outside every turn bracket).
--
-- Background: when a primary user-prompt request errors mid-stream,
-- `close_aborted_request` closes the predecessor's open turn as
-- 'abandoned' but the §3e turn-open lifecycle (which runs in
-- `snapshot_request` at settle) never fires — the errored prompt gets no
-- turn of its own. The Rust fix now opens the replacement turn inside the
-- abort path; this migration repairs the rows captured before the fix
-- (7 found on live data).
--
-- For each errored primary user-prompt request with no turn anchored at it
-- and no turn membership, create a CLOSED turn (outcome 'abandoned' — the
-- same label the abort path stamps) spanning from the request to the last
-- request before the next turn boundary in its session. The boundary set
-- includes both existing turn starts AND later orphan candidates, so two
-- consecutive errored prompts split into two turns rather than one
-- swallowing the other.

WITH cand AS (
    SELECT r.id, r.session_id, r.project_id, r.human_prompt, r.completed_at
    FROM requests.requests r
    JOIN requests.turn_types tt ON tt.id = r.turn_type_id AND tt.code = 'user_prompt'
    JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id AND ro.code IN ('primary', 'primary_first')
    JOIN requests.statuses s ON s.id = r.status_id AND s.code = 'error'
    WHERE NOT EXISTS (
        SELECT 1 FROM memory.operator_turns t WHERE t.start_request_id = r.id
    )
), bounded AS (
    SELECT c.*,
        -- Next boundary after this request: an existing turn start or a
        -- later orphan candidate in the same session.
        (
            SELECT MIN(b.boundary) FROM (
                SELECT t.start_request_id AS boundary
                FROM memory.operator_turns t
                WHERE t.session_id = c.session_id AND t.start_request_id > c.id
                UNION ALL
                SELECT c2.id FROM cand c2
                WHERE c2.session_id = c.session_id AND c2.id > c.id
            ) b
        ) AS next_boundary
    FROM cand c
), spans AS (
    SELECT b.*,
        (
            SELECT MAX(r2.id) FROM requests.requests r2
            WHERE r2.session_id = b.session_id
              AND r2.id >= b.id
              AND (b.next_boundary IS NULL OR r2.id < b.next_boundary)
        ) AS end_request_id
    FROM bounded b
)
INSERT INTO memory.operator_turns (
    session_id, project_id,
    start_request_id, end_request_id,
    user_prompt,
    outcome_id,
    closed_at, classified_at
)
SELECT s.session_id, s.project_id,
       s.id, s.end_request_id,
       s.human_prompt,
       (SELECT id FROM memory.turn_outcomes WHERE code = 'abandoned'),
       COALESCE(s.completed_at, NOW()), NOW()
FROM spans s;
