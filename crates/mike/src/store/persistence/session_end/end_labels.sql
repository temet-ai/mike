WITH target AS (
    SELECT unnest($1::bigint[]) AS turn_id
),
still_end AS (
    SELECT f.operator_turn_id AS turn_id, f.session_id
      FROM memory.turn_facts f
      JOIN target t ON t.turn_id = f.operator_turn_id
     WHERE f.next_genuine_at IS NULL OR f.next_genuine_at > $2
),
gold AS (
    SELECT DISTINCT ON (l.subject_id) l.subject_id AS turn_id, l.id AS label_id, l.label
      FROM eval.labels_as_of($2) l
      JOIN still_end e ON e.turn_id = l.subject_id
     WHERE l.subject_kind = 'session_end' AND l.axis = 'session_end' AND l.tier = 'gold'
     ORDER BY l.subject_id, l.created_at DESC, l.id DESC
),
live AS (
    SELECT DISTINCT ON (v.end_turn_id) v.end_turn_id AS turn_id, v.id AS verdict_id, v.end_label,
           v.end_source, v.end_rule, v.end_confidence, v.failure_tags, v.severity
      FROM session_judges.verdicts v
      JOIN still_end e ON e.turn_id = v.end_turn_id
     WHERE v.mode = 'live' AND v.end_label IS NOT NULL AND v.judged_at <= $2
     ORDER BY v.end_turn_id, v.judged_at DESC, v.id DESC
)
SELECT e.turn_id AS "end_turn_id!", e.session_id AS "session_id!",
       COALESCE(g.label, lv.end_label) AS "label!",
       CASE WHEN g.turn_id IS NOT NULL THEN 'operator'
            WHEN lv.end_source = 'deterministic' THEN 'deterministic'
            ELSE 'llm' END AS "source!",
       CASE WHEN g.turn_id IS NULL THEN lv.end_rule END AS "rule?",
       CASE WHEN g.turn_id IS NULL THEN lv.end_confidence END AS "confidence?",
       CASE WHEN g.turn_id IS NULL THEN lv.failure_tags ELSE '{}'::text[] END AS "failure_tags!",
       CASE WHEN g.turn_id IS NULL THEN lv.severity END AS "severity?",
       CASE WHEN g.turn_id IS NULL THEN lv.verdict_id END AS "verdict_id?",
       g.label_id AS "label_id?"
  FROM still_end e
  LEFT JOIN gold g ON g.turn_id = e.turn_id
  LEFT JOIN live lv ON lv.turn_id = e.turn_id
 WHERE g.turn_id IS NOT NULL OR lv.turn_id IS NOT NULL
 ORDER BY e.turn_id
