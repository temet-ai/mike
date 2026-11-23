SELECT s.id AS "statement_id!", s.subject_id AS "subject_id?", s.item_id AS "item_id!",
       (1 - (s.embedding <=> $2::text::public.vector))::float8 AS "similarity!"
  FROM memory.knowledge_statements s
  JOIN memory.knowledge_items i ON i.id = s.item_id
 WHERE s.state = 'current' AND (i.project_id = $1 OR i.project_id IS NULL) AND s.id <> ALL($3::bigint[])
 ORDER BY s.embedding <=> $2::text::public.vector
 LIMIT $4
