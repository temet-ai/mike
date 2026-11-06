WITH cur AS (
    SELECT DISTINCT ON (c.subject_id, c.axis) c.subject_id, c.axis, c.label
      FROM eval.labels_current_v c
     WHERE false::boolean AND c.subject_kind = 'turn' AND c.axis IN ('reaction', 'delivery')
       AND c.tier IN ('gold', 'relayed', 'seed')
       AND c.subject_id IN (SELECT operator_turn_id FROM eval.run_items WHERE run_id = 3::bigint)
     ORDER BY c.subject_id, c.axis,
              CASE c.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2 ELSE 3 END,
              c.created_at DESC, c.id DESC
), rev AS (
    SELECT l.operator_turn_id, max(l.created_at) AS reviewed_at
      FROM eval.labels l
     WHERE (false::boolean OR NULL::boolean IS NOT NULL)
       AND l.subject_kind = 'turn' AND l.tier = 'gold' AND l.action = 'assert'
       AND l.provenance ->> 'eval_run_id' = 3::bigint::text
     GROUP BY l.operator_turn_id
), x AS (
    SELECT i.*, rl.label AS gold_reaction, dl.label AS gold_delivery,
           coalesce(rl.provenance -> 'control' = 'true'::jsonb, false) AS gold_control,
           v.reaction AS verdict_reaction, v.delivery AS verdict_delivery,
           v.legacy_label AS verdict_legacy_label, f.next_state,
           CASE WHEN false::boolean THEN to_jsonb(v) END AS verdict,
           rl.severity AS review_severity, cr.label AS review_current_reaction,
           cd.label AS review_current_delivery, rev.reviewed_at AS review_at
      FROM eval.run_items i
      LEFT JOIN eval.labels rl ON rl.id = i.reaction_label_id
      LEFT JOIN eval.labels dl ON dl.id = i.delivery_label_id
      LEFT JOIN memory.turn_verdicts v ON v.id = i.turn_verdict_id
      LEFT JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
      LEFT JOIN cur cr ON cr.subject_id = i.operator_turn_id AND cr.axis = 'reaction'
      LEFT JOIN cur cd ON cd.subject_id = i.operator_turn_id AND cd.axis = 'delivery'
      LEFT JOIN rev ON rev.operator_turn_id = i.operator_turn_id
     WHERE i.run_id = 3::bigint
       AND (NULL::text IS NULL OR i.outcome = NULL::text)
       AND (false::boolean IS NULL OR i.reaction_correct = false::boolean)
       AND (NULL::boolean IS NULL OR i.delivery_correct = NULL::boolean)
       AND (NULL::text IS NULL
            OR (NULL::text = 'reaction' AND i.reaction_correct = false)
            OR (NULL::text = 'delivery' AND i.delivery_correct = false)
            OR (NULL::text = 'any' AND (i.reaction_correct = false OR i.delivery_correct = false)))
       AND (NULL::text IS NULL OR rl.label = NULL::text)
       AND (NULL::text IS NULL OR dl.label = NULL::text)
       AND (NULL::text IS NULL OR v.reaction = NULL::text)
       AND (NULL::text IS NULL OR v.delivery = NULL::text)
       AND (NULL::text IS NULL OR rl.severity = NULL::text)
       AND (NULL::boolean IS NULL OR (rev.reviewed_at IS NOT NULL) = NULL::boolean)
       AND (NULL::bigint IS NULL OR i.operator_turn_id = NULL::bigint)
)
SELECT x.*,
       CASE WHEN false::boolean THEN x.review_severity END AS gold_severity,
       CASE WHEN false::boolean THEN x.review_current_reaction END AS current_reaction,
       CASE WHEN false::boolean THEN x.review_current_delivery END AS current_delivery,
       CASE WHEN false::boolean THEN x.review_at END AS reviewed_at,
       count(*) OVER () AS total, count(x.review_at) OVER () AS reviewed_total
  FROM x
 ORDER BY x.operator_turn_id
 LIMIT 500::bigint OFFSET 0::bigint
