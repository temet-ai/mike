-- A busy steerer has ~10k unlabelled shadow rows, nearly all of them the last of their turn and
-- never picked, so every pass reads all of them (steerer 18 on 2026-11-19: 330 ms, 205k
-- buffers). Each candidate carries the evaluation columns it needs instead of being looked up
-- by id twice more; settled is materialized so the planner hashes the newest live pass per
-- (turn, question, trigger) instead of scanning live_top once per candidate under picked's
-- LIMIT; and verdict_json (about 2 kB, stored compressed) is decompressed once per row in vj
-- rather than once for each of the three keys picked reads.
WITH live_top AS MATERIALIZED (
    SELECT e.id, e.uuid, e.operator_turn_id, e.session_id, e.question, e.trigger_point,
           e.status, e.dispatched_at, e.verdict_json
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
    SELECT e.id, e.uuid, e.operator_turn_id, e.session_id, e.question, e.trigger_point,
           e.status, e.dispatched_at, e.verdict_json
      FROM steering.evaluations e
     WHERE e.steerer_id = 18
       AND e.status <> 'dispatched'
       AND (e.mode = 'shadow' OR e.arm = 'holdback')
       AND e.outcome_label IS NULL
    UNION ALL
    SELECT e.id, e.uuid, e.operator_turn_id, e.session_id, e.question, e.trigger_point,
           e.status, e.dispatched_at, e.verdict_json
      FROM steering.evaluations e
     WHERE e.steerer_id = 18
       AND e.status <> 'dispatched'
       AND e.arm = 'precheck_held'
       AND e.outcome_label IS NULL
    UNION ALL
    SELECT lt.id, lt.uuid, lt.operator_turn_id, lt.session_id, lt.question, lt.trigger_point,
           lt.status, lt.dispatched_at, lt.verdict_json
      FROM live_top lt
),
live_newest AS (
    SELECT DISTINCT ON (n.operator_turn_id, n.question, n.trigger_point)
           n.id, n.operator_turn_id, n.question, n.trigger_point, n.dispatched_at
      FROM live_top n
     ORDER BY n.operator_turn_id, n.question, n.trigger_point, n.dispatched_at DESC, n.id DESC
),
settled AS MATERIALIZED (
    SELECT c.*, f.next_genuine_request_id, f.next_genuine_at
      FROM cand c
     CROSS JOIN LATERAL (
         SELECT tf.next_genuine_request_id, tf.next_genuine_at
           FROM memory.turn_facts tf
          WHERE tf.operator_turn_id = c.operator_turn_id
            AND tf.next_state IN ('resolved', 'none_session_settled')
          LIMIT 1
     ) f
),
ranked AS (
    SELECT s.*, (newer_admitted.id IS NULL AND newer_live.id IS NULL) AS is_last
      FROM settled s
      LEFT JOIN LATERAL (
          SELECT n.id
            FROM steering.evaluations n
           WHERE n.operator_turn_id = s.operator_turn_id
             AND n.steerer_id = 18
             AND n.status <> 'dispatched'
             AND (n.mode = 'shadow' OR n.arm IN ('holdback', 'precheck_held'))
             AND n.question IS NOT DISTINCT FROM s.question
             AND n.trigger_point IS NOT DISTINCT FROM s.trigger_point
             AND (n.dispatched_at, n.id) > (s.dispatched_at, s.id)
           LIMIT 1
      ) newer_admitted ON true
      LEFT JOIN live_newest newer_live
        ON newer_live.operator_turn_id = s.operator_turn_id
       AND newer_live.question IS NOT DISTINCT FROM s.question
       AND newer_live.trigger_point IS NOT DISTINCT FROM s.trigger_point
       AND (newer_live.dispatched_at, newer_live.id) > (s.dispatched_at, s.id)
),
picked AS MATERIALIZED (
    SELECT k.uuid, k.operator_turn_id, k.session_id, k.question, k.trigger_point, k.status,
           k.dispatched_at, k.verdict_json, k.is_last,
           k.next_genuine_request_id, k.next_genuine_at
      FROM ranked k
     CROSS JOIN LATERAL (SELECT k.verdict_json || '{}'::jsonb AS v OFFSET 0) vj
     WHERE NOT k.is_last
        OR vj.v ->> 'exemption' IS NOT NULL
        OR (vj.v -> 'gates' -> k.question ->> 'open')::boolean
        OR (vj.v ->> 'recall_sample')::boolean
     ORDER BY k.dispatched_at DESC
     LIMIT 200
)
SELECT r.uuid AS "uuid!", r.operator_turn_id AS "operator_turn_id!", r.session_id AS "session_id!",
       r.question AS "question?", r.trigger_point AS "trigger_point?", r.status AS "status!",
       r.dispatched_at AS "dispatched_at!", r.verdict_json AS "verdict_json?", r.is_last AS "is_last!",
       f.next_state AS "next_state?", r.next_genuine_request_id AS "next_genuine_request_id?",
       EXTRACT(EPOCH FROM (r.next_genuine_at - t.closed_at))::bigint AS "next_genuine_after_secs?",
       nr.human_prompt AS "next_genuine_text?",
       t.reaction AS "reaction?", t.live_verdict_id AS "live_verdict_id?",
       t.final_assistant_text AS "final_assistant_text?", t.user_prompt AS "user_prompt?"
  FROM picked r
  JOIN memory.operator_turns t ON t.id = r.operator_turn_id
  JOIN memory.turn_facts f ON f.operator_turn_id = r.operator_turn_id
  LEFT JOIN requests.requests nr ON nr.id = r.next_genuine_request_id
 ORDER BY r.dispatched_at DESC
