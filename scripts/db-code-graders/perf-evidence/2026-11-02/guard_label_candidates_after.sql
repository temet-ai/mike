WITH live_top AS MATERIALIZED (
    SELECT e.id, e.operator_turn_id, e.question, e.trigger_point, e.dispatched_at
      FROM steering.evaluations e
     WHERE true
       AND e.steerer_id = 18
       AND e.status = 'resolved_pass'
       AND e.mode = 'live'
       AND e.arm IS NULL
       AND e.outcome_label IS NULL
     ORDER BY e.dispatched_at DESC
     LIMIT 200
),
cand AS (
    SELECT e.id
      FROM steering.evaluations e
     WHERE e.steerer_id = 18
       AND e.status <> 'dispatched'
       AND (e.mode = 'shadow' OR e.arm = 'holdback')
       AND e.outcome_label IS NULL
    UNION ALL
    SELECT lt.id FROM live_top lt
),
rows AS (
    SELECT e.uuid, e.operator_turn_id, e.session_id, e.question, e.trigger_point, e.status,
           e.dispatched_at, e.verdict_json,
           (newer_admitted.id IS NULL AND newer_live.id IS NULL) AS is_last
      FROM cand c
      JOIN steering.evaluations e ON e.id = c.id
      LEFT JOIN LATERAL (
          SELECT n.id
            FROM steering.evaluations n
           WHERE n.operator_turn_id = e.operator_turn_id
             AND n.steerer_id = 18
             AND n.status <> 'dispatched'
             AND (n.mode = 'shadow' OR n.arm = 'holdback')
             AND n.question IS NOT DISTINCT FROM e.question
             AND n.trigger_point IS NOT DISTINCT FROM e.trigger_point
             AND (n.dispatched_at, n.id) > (e.dispatched_at, e.id)
           LIMIT 1
      ) newer_admitted ON true
      LEFT JOIN LATERAL (
          SELECT n.id
            FROM live_top n
           WHERE n.operator_turn_id = e.operator_turn_id
             AND n.question IS NOT DISTINCT FROM e.question
             AND n.trigger_point IS NOT DISTINCT FROM e.trigger_point
             AND (n.dispatched_at, n.id) > (e.dispatched_at, e.id)
           LIMIT 1
      ) newer_live ON true
)
SELECT r.uuid, r.operator_turn_id, r.session_id, r.question, r.trigger_point, r.status,
       r.dispatched_at, r.verdict_json, r.is_last,
       f.next_state, f.next_genuine_request_id,
       EXTRACT(EPOCH FROM (f.next_genuine_at - t.closed_at))::bigint AS next_genuine_after_secs,
       nr.human_prompt AS next_genuine_text,
       t.reaction, t.live_verdict_id, t.final_assistant_text, t.user_prompt
  FROM rows r
  JOIN memory.operator_turns t ON t.id = r.operator_turn_id
  JOIN memory.turn_facts f ON f.operator_turn_id = r.operator_turn_id
  LEFT JOIN requests.requests nr ON nr.id = f.next_genuine_request_id
 WHERE f.next_state IN ('resolved', 'none_session_settled')
   AND (NOT r.is_last
        OR r.verdict_json ->> 'exemption' IS NOT NULL
        OR (r.verdict_json -> 'gates' -> r.question ->> 'open')::boolean
        OR (r.verdict_json ->> 'recall_sample')::boolean)
 ORDER BY r.dispatched_at DESC
 LIMIT 200;
