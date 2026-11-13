WITH node_set AS (
    SELECT se.session_id, se.embedding, se.source, s.id AS session_db_id, s.title, s.last_seen,
           s.last_status AS cached_status
      FROM memory.session_embeddings se
      JOIN sessions.sessions s ON s.session_uuid = se.session_id
     WHERE s.project_id = $1
     ORDER BY s.last_seen DESC
     LIMIT $2
)
SELECT n.session_id AS "session_id!",
       n.title AS "title?",
       lv.outcome AS "outcome?",
       n.last_seen AS "last_seen!",
       COALESCE(ns.live_status, n.cached_status) AS "last_status?",
       CASE WHEN pe.embedding IS NOT NULL
            THEN (1.0 - (n.embedding <=> pe.embedding))::float8
       END AS "similarity_to_centroid?",
       n.source AS "source!",
       COALESCE(sk.topics, '{}') AS "topics!"
  FROM node_set n
  LEFT JOIN LATERAL (
      SELECT o.code AS outcome
        FROM session_judges.verdicts v
        JOIN session_judges.outcomes o ON o.id = v.outcome_id
       WHERE v.session_id = n.session_db_id AND v.mode = 'live'
       ORDER BY v.judged_at DESC, v.id DESC
       LIMIT 1
  ) lv ON TRUE
  LEFT JOIN memory.session_knowledge sk ON sk.session_id = n.session_id
  LEFT JOIN memory.project_embeddings pe ON pe.project_id = $1
  LEFT JOIN LATERAL (
      -- Newest-request status, read live because sessions.last_status can go stale
      -- (issue #147). Top-1-per-group over the node set only.
      SELECT CASE st.code
                 WHEN 'pending'   THEN 'Pending'
                 WHEN 'streaming' THEN 'Streaming'
                 WHEN 'complete'  THEN 'Complete'
                 WHEN 'error'     THEN 'Error: ' || COALESCE(re.message, '')
             END AS live_status
        FROM requests.requests nr
        JOIN requests.statuses st ON st.id = nr.status_id
        LEFT JOIN requests.errors re ON re.request_id = nr.id
       WHERE nr.session_id = n.session_db_id
       ORDER BY nr.created_at DESC, nr.id DESC
       LIMIT 1
  ) ns ON TRUE
 ORDER BY n.last_seen DESC
