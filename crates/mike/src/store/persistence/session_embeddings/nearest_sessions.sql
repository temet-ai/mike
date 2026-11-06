WITH anchor AS (
    SELECT se.embedding, se.session_id
      FROM memory.session_embeddings se
     WHERE se.session_id = $1
),
dists AS MATERIALIZED (
    SELECT b.session_id, b.source, a.embedding <=> b.embedding AS distance
      FROM anchor a
      JOIN memory.session_embeddings b ON b.session_id <> a.session_id
),
ranked AS (
    SELECT d.session_id, d.source, (1.0 - d.distance)::float8 AS similarity, d.distance
      FROM dists d
     WHERE (1.0 - d.distance) >= $3
     ORDER BY d.distance ASC
     LIMIT $2
)
SELECT r.session_id AS "session_id!",
       s.title AS "title?",
       s.project_id AS "project_id!",
       p.project_path AS "project_path!",
       lv.outcome AS "outcome?",
       s.last_seen AS "last_seen!",
       COALESCE(ns.live_status, s.last_status) AS "last_status?",
       r.similarity AS "similarity!",
       r.source AS "source!",
       COALESCE(sk.topics, '{}') AS "topics!"
  FROM ranked r
  JOIN sessions.sessions s ON s.session_uuid = r.session_id
  JOIN projects.projects p ON p.id = s.project_id
  LEFT JOIN LATERAL (
      SELECT o.code AS outcome
        FROM session_judges.verdicts v
        JOIN session_judges.outcomes o ON o.id = v.outcome_id
       WHERE v.session_id = s.id AND v.mode = 'live'
       ORDER BY v.judged_at DESC, v.id DESC
       LIMIT 1
  ) lv ON TRUE
  LEFT JOIN memory.session_knowledge sk ON sk.session_id = r.session_id
  LEFT JOIN LATERAL (
      -- Newest-request status, read live because sessions.last_status can go stale
      -- (issue #147). Top-1-per-group over the ranked rows only.
      SELECT CASE st.code
                 WHEN 'pending'   THEN 'Pending'
                 WHEN 'streaming' THEN 'Streaming'
                 WHEN 'complete'  THEN 'Complete'
                 WHEN 'error'     THEN 'Error: ' || COALESCE(re.message, '')
             END AS live_status
        FROM requests.requests nr
        JOIN requests.statuses st ON st.id = nr.status_id
        LEFT JOIN requests.errors re ON re.request_id = nr.id
       WHERE nr.session_id = s.id
       ORDER BY nr.created_at DESC, nr.id DESC
       LIMIT 1
  ) ns ON TRUE
 ORDER BY r.distance ASC
