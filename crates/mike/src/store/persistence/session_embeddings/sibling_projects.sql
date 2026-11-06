SELECT p.id AS project_id,
       p.project_path AS "project_path!",
       (1.0 - (a.embedding <=> b.embedding))::float8 AS "similarity!"
  FROM memory.project_embeddings a
  JOIN memory.project_embeddings b ON b.project_id <> a.project_id
  JOIN projects.projects p ON p.id = b.project_id
 WHERE a.project_id = $1
 ORDER BY a.embedding <=> b.embedding ASC
 LIMIT $2
