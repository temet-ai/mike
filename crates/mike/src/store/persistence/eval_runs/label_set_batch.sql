WITH l AS (
    SELECT l.subject_id, l.axis, l.operator_turn_id, l.id, l.label, l.inclusion_prob,
           l.provenance, l.created_at,
           CASE l.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2 WHEN 'seed' THEN 3 ELSE 4 END AS tier_rank
      FROM eval.labels_as_of($1) l
      JOIN eval.import_batches ib ON ib.id = l.import_batch_id AND ib.source_key = $5
     WHERE l.subject_kind = 'turn'
       AND l.axis IN ('reaction', 'delivery')
       AND l.split = $2
       AND l.tier = ANY($3::text[])
       AND l.method = ANY($4::text[])
       AND ($2 <> 'holdout' OR coalesce(l.provenance ->> 'purpose', '') <> 'holdout_spot_check')
),
best AS (
    SELECT DISTINCT ON (subject_id, axis)
           subject_id, axis, operator_turn_id, id, label, inclusion_prob, provenance
      FROM l
     ORDER BY subject_id, axis, tier_rank, created_at DESC, id DESC
)
SELECT r.operator_turn_id AS "operator_turn_id!", r.id AS "reaction_label_id!",
       r.label AS "reaction_label!", d.id AS "delivery_label_id?", d.label AS "delivery_label?",
       r.inclusion_prob AS "inclusion_prob?",
       coalesce(r.provenance ->> 'stratum', 'all') AS "stratum!",
       coalesce(r.provenance -> 'control' = 'true'::jsonb, false) AS "control!"
  FROM best r
  LEFT JOIN best d ON d.subject_id = r.subject_id AND d.axis = 'delivery'
 WHERE r.axis = 'reaction'
 ORDER BY r.operator_turn_id
