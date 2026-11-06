WITH node_set AS (
    SELECT se.session_id, se.embedding
      FROM memory.session_embeddings se
      JOIN sessions.sessions s ON s.session_uuid = se.session_id
     WHERE s.project_id = $1
     ORDER BY s.last_seen DESC
     LIMIT $2
),
ranked AS (
    SELECT a.session_id AS from_id,
           b.session_id AS to_id,
           (1.0 - (a.embedding <=> b.embedding))::float8 AS similarity,
           ROW_NUMBER() OVER (
               PARTITION BY a.session_id
               ORDER BY a.embedding <=> b.embedding ASC
           ) AS rn
      FROM node_set a
      JOIN node_set b ON b.session_id <> a.session_id
     WHERE (1.0 - (a.embedding <=> b.embedding)) >= $3
)
SELECT LEAST(from_id, to_id)    AS "from_session_id!",
       GREATEST(from_id, to_id) AS "to_session_id!",
       MAX(similarity)          AS "similarity!"
  FROM ranked
 WHERE rn <= $4
 GROUP BY LEAST(from_id, to_id), GREATEST(from_id, to_id)
