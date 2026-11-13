WITH target AS (
    SELECT DISTINCT unnest($1::bigint[]) AS turn_id
),
v2 AS (
    SELECT DISTINCT ON (v.operator_turn_id)
           v.operator_turn_id AS turn_id, v.id, v.bundle_id, v.reaction, v.delivery, v.created_at
      FROM memory.turn_verdicts v
      JOIN target t ON t.turn_id = v.operator_turn_id
     WHERE v.mode = 'live' AND v.reaction IS NOT NULL AND v.created_at <= $2
     ORDER BY v.operator_turn_id, v.created_at DESC, v.id DESC
),
r3 AS (
    SELECT DISTINCT ON (v.operator_turn_id)
           v.operator_turn_id AS turn_id, v.id, v.bundle_id, v.legacy_label, v.created_at
      FROM memory.turn_verdicts v
      JOIN target t ON t.turn_id = v.operator_turn_id
     WHERE v.mode = 'live' AND v.reaction IS NULL AND v.legacy_label IS NOT NULL
       AND v.created_at <= $2
     ORDER BY v.operator_turn_id, v.created_at DESC, v.id DESC
)
SELECT t.turn_id AS "turn_id!",
       COALESCE(a.id, b.id) AS "verdict_id!",
       CASE WHEN a.id IS NOT NULL THEN a.bundle_id ELSE b.bundle_id END AS "bundle_id?",
       (a.id IS NOT NULL) AS "is_v2!",
       a.reaction AS "reaction?",
       a.delivery AS "delivery?",
       CASE WHEN a.id IS NULL THEN b.legacy_label END AS "legacy_label?",
       COALESCE(a.created_at, b.created_at) AS "created_at!"
  FROM target t
  LEFT JOIN v2 a ON a.turn_id = t.turn_id
  LEFT JOIN r3 b ON b.turn_id = t.turn_id
 WHERE a.id IS NOT NULL OR b.id IS NOT NULL
 ORDER BY t.turn_id
