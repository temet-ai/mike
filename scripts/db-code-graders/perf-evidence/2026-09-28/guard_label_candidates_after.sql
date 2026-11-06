WITH rows AS (
    SELECT e.uuid, e.operator_turn_id, e.session_id, e.question, e.trigger_point, e.status,
           e.dispatched_at, e.verdict_json,
           row_number() OVER (PARTITION BY e.operator_turn_id, e.question, e.trigger_point
                              ORDER BY e.dispatched_at DESC, e.id DESC) AS recency
      FROM steering.evaluations e
     WHERE e.steerer_id = 18
       AND (e.mode = 'shadow' OR e.arm = 'holdback'
            OR (true AND e.mode = 'live' AND e.status = 'resolved_pass' AND e.arm IS NULL))
       AND e.status <> 'dispatched'
)
SELECT r.uuid, r.operator_turn_id, r.session_id, r.question, r.trigger_point, r.status,
       r.dispatched_at, r.verdict_json, (r.recency = 1) AS is_last,
       f.next_state, f.next_genuine_request_id,
       EXTRACT(EPOCH FROM (f.next_genuine_at - t.closed_at))::bigint AS next_genuine_after_secs,
       nr.human_prompt AS next_genuine_text,
       t.reaction, t.live_verdict_id, t.final_assistant_text, t.user_prompt
  FROM rows r
  JOIN steering.evaluations e ON e.uuid = r.uuid AND e.outcome_label IS NULL
  JOIN memory.operator_turns t ON t.id = r.operator_turn_id
  JOIN memory.turn_facts f ON f.operator_turn_id = r.operator_turn_id
  LEFT JOIN requests.requests nr ON nr.id = f.next_genuine_request_id
 WHERE f.next_state IN ('resolved', 'none_session_settled')
   AND (r.recency > 1
        OR r.verdict_json ->> 'exemption' IS NOT NULL
        OR (r.verdict_json -> 'gates' -> r.question ->> 'open')::boolean
        OR (r.verdict_json ->> 'recall_sample')::boolean)
 ORDER BY r.dispatched_at DESC
 LIMIT 200;
