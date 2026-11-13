WITH scope AS (
    SELECT t.id AS turn_id, t.session_id, t.start_request_id, t.end_request_id,
           t.outcome_evidence_request_id, t.closed_at,
           lead(t.start_request_id) OVER (
               PARTITION BY t.session_id ORDER BY t.start_request_id
           ) AS next_start
      FROM memory.operator_turns t
     WHERE t.session_id IN (SELECT s.session_id FROM memory.operator_turns s WHERE s.id = ANY($1))
), wanted AS (
    SELECT turn_id, session_id, start_request_id, end_request_id, outcome_evidence_request_id, closed_at, next_start
      FROM scope WHERE turn_id = ANY($1)
), stop AS (
    SELECT DISTINCT ON (w.turn_id)
           w.turn_id, r.id AS stop_request_id,
           CASE WHEN resp.id IS NULL THEN 'aborted' ELSE rsr.code END AS stop_reason
      FROM wanted w
      JOIN requests.requests r
        ON r.session_id = w.session_id
       AND r.id >= w.start_request_id
       AND (w.next_start IS NULL OR r.id < w.next_start)
      JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
                                   AND ro.code IN ('primary', 'primary_first')
      LEFT JOIN responses.responses resp ON resp.request_id = r.id
      LEFT JOIN responses.stop_reasons rsr ON rsr.id = resp.stop_reason_id
     WHERE EXISTS (
            SELECT 1
              FROM responses.stream_blocks sb
              JOIN responses.stream_block_types sbt ON sbt.id = sb.block_type_id
             WHERE sb.request_id = r.id AND sbt.code = 'text')
     ORDER BY w.turn_id, r.id DESC
), interrupted AS (
    SELECT DISTINCT w.turn_id
      FROM wanted w
      JOIN requests.requests r
        ON r.session_id = w.session_id
       AND r.id > w.start_request_id
       AND r.id <= COALESCE(w.outcome_evidence_request_id, w.end_request_id)
      JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
                                   AND ro.code IN ('primary', 'primary_first')
     WHERE strpos(r.human_prompt, '[Request interrupted by user') > 0
)
SELECT w.turn_id AS "turn_id!",
       stop.stop_request_id,
       stop.stop_reason,
       (interrupted.turn_id IS NOT NULL) AS "interrupted!",
       CASE
           WHEN w.outcome_evidence_request_id IS NOT NULL THEN 'next_prompt'
           WHEN EXISTS (
                SELECT 1
                  FROM requests.requests ar
                  JOIN requests.errors e ON e.request_id = ar.id
                 WHERE ar.session_id = w.session_id
                   AND ar.completed_at = w.closed_at
                   AND e.message = 'stream aborted') THEN 'abort'
           ELSE 'idle_reap'
       END AS "close_cause!"
  FROM wanted w
  LEFT JOIN stop ON stop.turn_id = w.turn_id
  LEFT JOIN interrupted ON interrupted.turn_id = w.turn_id
 ORDER BY w.turn_id
