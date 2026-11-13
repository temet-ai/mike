WITH rows AS (
    SELECT e.uuid, e.session_id, e.operator_turn_id, e.question, e.trigger_point, e.status,
           e.outcome_label, e.label_evidence, e.verdict_json,
           row_number() OVER (PARTITION BY e.operator_turn_id, e.question, e.trigger_point
                              ORDER BY e.dispatched_at DESC, e.id DESC) AS recency
      FROM steering.evaluations e
     WHERE e.steerer_id = $1
       AND e.dispatched_at >= $2
       AND e.dispatched_at < $3
)
SELECT uuid AS "uuid!", session_id AS "session_id!", operator_turn_id AS "operator_turn_id!", question AS "question?", trigger_point AS "trigger_point?", status AS "status!", outcome_label AS "outcome_label?",
       verdict_json ->> 'exemption' AS "exemption?",
       verdict_json -> 'gates' -> question ->> 'arm' AS "arm?",
       COALESCE((verdict_json -> 'gates' -> question ->> 'open')::boolean, false) AS "gate_open!",
       verdict_json -> 'gates' -> question ->> 'reason' AS "gate_reason?",
       COALESCE((verdict_json -> 'gates' -> question -> 'other_arms' -> 'g2' ->> 'open')::boolean, false)
           AS "other_g2_open!",
       COALESCE((verdict_json -> 'gates' -> question -> 'other_arms' -> 'g2b' ->> 'open')::boolean, false)
           AS "other_g2b_open!",
       verdict_json ->> 'abstention' AS "abstention?",
       COALESCE((verdict_json ->> 'recall_sample')::boolean, false) AS "recall_sample!",
       (verdict_json -> 'first_vote' -> question ->> 'fires')::boolean AS "first_vote_fires?",
       COALESCE((verdict_json -> 'reach' ->> 'stream_open')::boolean, false) AS "stream_open!",
       COALESCE((verdict_json -> 'reach' ->> 'channel_live')::boolean, false) AS "channel_live!",
       (label_evidence ->> 'next_genuine_after_secs')::bigint AS "next_genuine_after_secs?",
       (recency > 1) AS "superseded!"
  FROM rows
 ORDER BY operator_turn_id, question, trigger_point
