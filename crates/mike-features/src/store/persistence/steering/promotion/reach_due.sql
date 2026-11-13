WITH due AS (
    SELECT e.id, e.session_id, e.operator_turn_id, e.trigger_request_id, e.question
      FROM steering.evaluations e
     WHERE e.due_at IS NOT NULL AND e.reach_at_due IS NULL AND e.due_at <= now()
       AND e.arm = 'holdback'
     ORDER BY e.due_at
     LIMIT $1
),
due_sessions AS (
    SELECT DISTINCT session_id FROM due
),
streams AS (
    SELECT ch.session_id,
           bool_or(ch.stream_opened_at <= now()
                   AND (ch.stream_closed_at IS NULL OR ch.stream_closed_at > now())
                   AND ch.last_seen_at >= now() - make_interval(secs => 2 * COALESCE(ch.last_heartbeat_interval_seconds, 60)))
               AS open
      FROM remote_control.channels ch
      JOIN due_sessions ds ON ds.session_id = ch.session_id
     GROUP BY ch.session_id
)
SELECT d.id AS "id!", d.session_id AS "session_id!", d.question AS "question?",
       (t.closed_at IS NOT NULL) AS "turn_closed!",
       EXISTS (
           SELECT 1
             FROM requests.requests r
             JOIN requests.turn_origins o ON o.id = r.turn_origin_id
            WHERE r.session_id = d.session_id
              AND r.created_at > tr.created_at
              AND o.code IN ('primary', 'primary_first')
       ) AS "moved_on!",
       COALESCE(st.open, false) AS "stream_open!"
  FROM due d
  LEFT JOIN memory.operator_turns t ON t.id = d.operator_turn_id
  LEFT JOIN requests.requests tr ON tr.id = d.trigger_request_id
  LEFT JOIN streams st ON st.session_id = d.session_id
 ORDER BY d.id
