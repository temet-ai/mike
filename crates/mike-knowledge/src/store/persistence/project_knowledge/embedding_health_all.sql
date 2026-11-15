SELECT count(*) AS "total!",
       count(*) FILTER (WHERE NOT i.needs_embedding AND e.model_key = ANY($1::text[])) AS "ready!",
       count(*) FILTER (WHERE i.last_error IS NOT NULL) AS "failed!",
       count(*) FILTER (WHERE NOT i.needs_embedding AND e.model_key = $2) AS "current_version!"
  FROM memory.knowledge_items i
  LEFT JOIN memory.knowledge_embedding_state e ON e.item_id = i.id
