SELECT a.session_id AS "a!",
       b.session_id AS "b!",
       (1.0 - (a.embedding <=> b.embedding))::float8 AS "similarity!"
  FROM memory.session_embeddings a
  JOIN memory.session_embeddings b ON a.session_id < b.session_id
 WHERE a.session_id = ANY($1) AND b.session_id = ANY($1)
