-- A busy steerer has ~10k unlabelled shadow rows, nearly all of them the last of their turn and
-- never picked, so every pass reads all of them. Each candidate carries the evaluation columns it
-- needs instead of being looked up by id again; the newest admitted pass per (turn, question,
-- trigger) is read for all candidate turns in one index scan over an array of turn ids rather than
-- one probe per turn; verdict_json (about 2 kB, stored compressed) is decompressed once per row in
-- vj; and the turn facts are looked up only for the rows that pass the pick filter, not for every
-- candidate (steerer 18 on 2026-11-20: 75k -> 39k buffers with the same rows).
WITH live_top AS MATERIALIZED (
    SELECT e.id, e.uuid, e.operator_turn_id, e.session_id, e.question, e.trigger_point,
           e.status, e.dispatched_at, e.verdict_json
      FROM steering.evaluations e
     WHERE $3
       AND e.steerer_id = $1
       AND e.status = 'resolved_pass'
       AND e.mode = 'live'
       AND e.arm IS NULL
       AND e.outcome_label IS NULL
     ORDER BY e.dispatched_at DESC
     LIMIT $2
),
cand AS MATERIALIZED (
    SELECT e.id, e.uuid, e.operator_turn_id, e.session_id, e.question, e.trigger_point,
           e.status, e.dispatched_at, e.verdict_json
      FROM steering.evaluations e
     WHERE e.steerer_id = $1
       AND e.status <> 'dispatched'
       AND (e.mode = 'shadow' OR e.arm = 'holdback')
       AND e.outcome_label IS NULL
    UNION ALL
    SELECT e.id, e.uuid, e.operator_turn_id, e.session_id, e.question, e.trigger_point,
           e.status, e.dispatched_at, e.verdict_json
      FROM steering.evaluations e
     WHERE e.steerer_id = $1
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
admitted_newest AS (
    SELECT DISTINCT ON (n.operator_turn_id, n.question, n.trigger_point)
           n.id, n.operator_turn_id, n.question, n.trigger_point, n.dispatched_at
      FROM steering.evaluations n
     WHERE n.steerer_id = $1
       AND n.status <> 'dispatched'
       AND (n.mode = 'shadow' OR n.arm IN ('holdback', 'precheck_held'))
       AND n.operator_turn_id = ANY (ARRAY(SELECT DISTINCT c.operator_turn_id FROM cand c))
     ORDER BY n.operator_turn_id, n.question, n.trigger_point, n.dispatched_at DESC, n.id DESC
),
ranked AS (
    SELECT c.*, (newer_admitted.id IS NULL AND newer_live.id IS NULL) AS is_last
      FROM cand c
      LEFT JOIN admitted_newest newer_admitted
        ON newer_admitted.operator_turn_id = c.operator_turn_id
       AND newer_admitted.question IS NOT DISTINCT FROM c.question
       AND newer_admitted.trigger_point IS NOT DISTINCT FROM c.trigger_point
       AND (newer_admitted.dispatched_at, newer_admitted.id) > (c.dispatched_at, c.id)
      LEFT JOIN live_newest newer_live
        ON newer_live.operator_turn_id = c.operator_turn_id
       AND newer_live.question IS NOT DISTINCT FROM c.question
       AND newer_live.trigger_point IS NOT DISTINCT FROM c.trigger_point
       AND (newer_live.dispatched_at, newer_live.id) > (c.dispatched_at, c.id)
),
pickable AS MATERIALIZED (
    SELECT k.*
      FROM ranked k
     CROSS JOIN LATERAL (SELECT k.verdict_json || '{}'::jsonb AS v OFFSET 0) vj
     WHERE NOT k.is_last
        OR vj.v ->> 'exemption' IS NOT NULL
        OR (vj.v -> 'gates' -> k.question ->> 'open')::boolean
        OR (vj.v ->> 'recall_sample')::boolean
),
picked AS MATERIALIZED (
    SELECT k.uuid, k.operator_turn_id, k.session_id, k.question, k.trigger_point, k.status,
           k.dispatched_at, k.verdict_json, k.is_last,
           f.next_state, f.next_genuine_request_id, f.next_genuine_at
      FROM pickable k
     CROSS JOIN LATERAL (
         SELECT tf.next_state, tf.next_genuine_request_id, tf.next_genuine_at
           FROM memory.turn_facts tf
          WHERE tf.operator_turn_id = k.operator_turn_id
            AND tf.next_state IN ('resolved', 'none_session_settled')
          LIMIT 1
     ) f
     ORDER BY k.dispatched_at DESC
     LIMIT $2
)
SELECT r.uuid AS "uuid!", r.operator_turn_id AS "operator_turn_id!", r.session_id AS "session_id!",
       r.question AS "question?", r.trigger_point AS "trigger_point?", r.status AS "status!",
       r.dispatched_at AS "dispatched_at!", r.verdict_json AS "verdict_json?", r.is_last AS "is_last!",
       r.next_state AS "next_state?", r.next_genuine_request_id AS "next_genuine_request_id?",
       EXTRACT(EPOCH FROM (r.next_genuine_at - t.closed_at))::bigint AS "next_genuine_after_secs?",
       nr.human_prompt AS "next_genuine_text?",
       t.reaction AS "reaction?", t.live_verdict_id AS "live_verdict_id?",
       t.final_assistant_text AS "final_assistant_text?", t.user_prompt AS "user_prompt?"
  FROM picked r
  JOIN memory.operator_turns t ON t.id = r.operator_turn_id
  LEFT JOIN requests.requests nr ON nr.id = r.next_genuine_request_id
 ORDER BY r.dispatched_at DESC
