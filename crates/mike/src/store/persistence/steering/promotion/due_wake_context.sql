WITH claimed AS (
    SELECT i.id, i.uuid, i.session_id, i.trigger_request_id, i.expires_at, i.delivered_request_id,
           i.reverted_at, i.revert_reason, i.payload_text, i.operator_turn_id
      FROM steering.interventions i
     WHERE i.id = ANY($1::bigint[])
),
claimed_sessions AS (
    SELECT DISTINCT session_id FROM claimed
),
pushed AS (
    SELECT i.session_id, count(*) AS n
      FROM steering.interventions i
      JOIN claimed_sessions cs ON cs.session_id = i.session_id
     WHERE i.wake_state = 'pushed' AND i.wake_resolved_at > now() - interval '1 hour'
     GROUP BY i.session_id
),
streams AS (
    SELECT ch.session_id,
           bool_or(ch.stream_opened_at <= now()
                   AND (ch.stream_closed_at IS NULL OR ch.stream_closed_at > now())
                   AND ch.last_seen_at >= now() - make_interval(secs => 2 * COALESCE(ch.last_heartbeat_interval_seconds, 60)))
               AS open
      FROM remote_control.channels ch
      JOIN claimed_sessions cs ON cs.session_id = ch.session_id
     GROUP BY ch.session_id
)
SELECT c.id AS "id!", c.uuid AS "uuid!", c.session_id AS "session_id!", s.session_uuid AS "session_uuid!",
       e.question AS "question?", e.arm_anchor_turn_id AS "anchor_turn_id?",
       e.id AS "evaluation_id!", e.operator_turn_id AS "evaluated_turn_id!", p.mode AS "mode?",
       c.expires_at AS "expires_at!",
       (c.delivered_request_id IS NOT NULL) AS "delivered!", (c.reverted_at IS NOT NULL) AS "reverted!",
       c.revert_reason AS "revert_reason?", COALESCE(c.payload_text, '') AS "payload_text!",
       (t.closed_at IS NOT NULL) AS "turn_closed!",
       EXISTS (
           SELECT 1
             FROM requests.requests r
             JOIN requests.turn_origins o ON o.id = r.turn_origin_id
            WHERE r.session_id = c.session_id
              AND r.created_at > tr.created_at
              AND o.code IN ('primary', 'primary_first')
       ) AS "moved_on!",
       COALESCE(st.open, false) AS "stream_open!",
       COALESCE(pu.n, 0)::bigint AS "pushed_last_hour!"
  FROM claimed c
  JOIN sessions.sessions s ON s.id = c.session_id
  JOIN steering.evaluations e ON e.intervention_uuid = c.uuid
  LEFT JOIN steering.question_policies p ON p.steerer_id = e.steerer_id AND p.question = e.question
  LEFT JOIN memory.operator_turns t ON t.id = e.operator_turn_id
  LEFT JOIN requests.requests tr ON tr.id = c.trigger_request_id
  LEFT JOIN streams st ON st.session_id = c.session_id
  LEFT JOIN pushed pu ON pu.session_id = c.session_id
 ORDER BY c.id
