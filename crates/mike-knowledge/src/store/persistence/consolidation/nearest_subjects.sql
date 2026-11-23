SELECT n.id AS "subject_id!", n.similarity AS "similarity!"
  FROM memory.knowledge_subjects me
  CROSS JOIN LATERAL (
       SELECT o.id, (1 - (o.centre <=> me.centre))::float8 AS similarity
         FROM memory.knowledge_subjects o
        WHERE o.id <> me.id AND o.members > 0 AND o.project_id IS NOT DISTINCT FROM me.project_id
        ORDER BY o.centre <=> me.centre
        LIMIT $2) n
 WHERE me.id = $1
 ORDER BY n.similarity DESC, n.id
