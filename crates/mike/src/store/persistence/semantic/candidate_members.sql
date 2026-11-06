SELECT pc.id AS candidate_id, m.item_id AS "item_id!", m.ord AS "ord!", i.text, i.session_id,
       i.operator_turn_id, i.effective_at,
       (1 - (i.embedding <=> md.embedding))::float8 AS similarity_to_medoid
  FROM memory.pattern_candidates pc
  CROSS JOIN LATERAL unnest(pc.member_item_ids) WITH ORDINALITY AS m(item_id, ord)
  LEFT JOIN memory.semantic_items i ON i.id = m.item_id
  LEFT JOIN memory.semantic_items md ON md.id = pc.medoid_item_id
 WHERE pc.id = ANY($1)
 ORDER BY pc.id, m.ord
