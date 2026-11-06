WITH arms AS (
    SELECT e.id, e.uuid, e.session_id, e.operator_turn_id, e.question, e.arm, e.holdback_draw,
           e.arm_anchor_turn_id, e.due_at, e.reach_at_due, e.dispatched_at, e.intervention_uuid,
           e.outcome_label, e.label_source
      FROM steering.evaluations e
     WHERE e.steerer_id = $1
       AND e.arm IS NOT NULL
       AND e.question = $2
       AND e.dispatched_at <= $3
),
carrier AS (
    SELECT DISTINCT ON (a.id) a.id, r.id AS carrier_request_id, r.operator_turn_id AS carrier_turn_id
      FROM arms a
      JOIN requests.requests r ON r.session_id = a.session_id
                              AND r.created_at > a.dispatched_at
                              AND r.created_at <= $3
      JOIN requests.turn_origins o ON o.id = r.turn_origin_id AND o.code IN ('primary', 'primary_first')
     ORDER BY a.id, r.created_at, r.id
),
scored AS (
    SELECT a.id, a.uuid, a.session_id, a.operator_turn_id, a.question, a.arm, a.holdback_draw,
           a.arm_anchor_turn_id, a.due_at, a.reach_at_due, a.dispatched_at, a.intervention_uuid,
           a.outcome_label, a.label_source, c.carrier_request_id,
           CASE WHEN a.reach_at_due = 'reachable' THEN a.operator_turn_id ELSE c.carrier_turn_id END
               AS scored_turn_id
      FROM arms a
      LEFT JOIN carrier c ON c.id = a.id
),
reactions AS (
    SELECT DISTINCT ON (v.operator_turn_id) v.operator_turn_id, v.reaction
      FROM memory.turn_verdicts v
      JOIN scored s ON s.scored_turn_id = v.operator_turn_id
     WHERE v.mode = 'live' AND v.created_at <= $3
     ORDER BY v.operator_turn_id, v.created_at DESC, v.id DESC
),
arm_sessions AS (
    SELECT DISTINCT session_id FROM arms
),
last_turns AS (
    SELECT t.session_id, max(t.id) AS last_turn_id
      FROM memory.operator_turns t
      JOIN arm_sessions s ON s.session_id = t.session_id
     WHERE t.created_at <= $3
     GROUP BY t.session_id
)
SELECT s.id AS "id!", s.uuid AS "uuid!", s.session_id AS "session_id!",
       s.operator_turn_id AS "operator_turn_id!", s.question AS "question?", s.arm AS "arm?",
       s.holdback_draw AS "holdback_draw?", s.arm_anchor_turn_id AS "arm_anchor_turn_id?",
       s.due_at AS "due_at?", s.reach_at_due AS "reach_at_due?", s.dispatched_at AS "dispatched_at!",
       s.outcome_label AS "outcome_label?", s.label_source AS "label_source?",
       s.carrier_request_id AS "carrier_request_id?", s.scored_turn_id AS "scored_turn_id?",
       rx.reaction AS "scored_reaction?",
       COALESCE(s.scored_turn_id IS NOT NULL AND s.scored_turn_id = lt.last_turn_id, false) AS "scored_is_last!",
       (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL) AS "delivered!",
       i.wake_state AS "wake_state?"
  FROM scored s
  LEFT JOIN reactions rx ON rx.operator_turn_id = s.scored_turn_id
  LEFT JOIN last_turns lt ON lt.session_id = s.session_id
  LEFT JOIN steering.interventions i ON i.uuid = s.intervention_uuid
 ORDER BY s.session_id, s.dispatched_at, s.id
