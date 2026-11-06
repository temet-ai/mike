SELECT i.*, rl.label AS gold_reaction, dl.label AS gold_delivery,
        coalesce(rl.provenance -> 'control' = 'true'::jsonb, false) AS gold_control,
        v.reaction AS verdict_reaction, v.delivery AS verdict_delivery,
        v.legacy_label AS verdict_legacy_label, f.next_state,
        CASE WHEN false THEN to_jsonb(v) END AS verdict,
        count(*) OVER () AS total
   FROM eval.run_items i
   LEFT JOIN eval.labels rl ON rl.id = i.reaction_label_id
   LEFT JOIN eval.labels dl ON dl.id = i.delivery_label_id
   LEFT JOIN memory.turn_verdicts v ON v.id = i.turn_verdict_id
   LEFT JOIN memory.turn_facts f ON f.operator_turn_id = i.operator_turn_id
  WHERE i.run_id = 3
    AND (NULL::text IS NULL OR i.outcome = NULL::text)
    AND (false IS NULL OR i.reaction_correct = false)
  ORDER BY i.operator_turn_id
  LIMIT 500 OFFSET 0
