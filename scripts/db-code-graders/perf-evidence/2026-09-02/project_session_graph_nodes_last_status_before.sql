WITH node_set AS (
     SELECT se.session_id, se.embedding, se.source, s.title, s.last_seen
       FROM memory.session_embeddings se
       JOIN sessions.sessions s ON s.session_uuid = se.session_id
      WHERE s.project_id = 313870
      ORDER BY s.last_seen DESC
      LIMIT 60
 ),
 latest_verdict AS (
     SELECT DISTINCT ON (v.session_id) v.session_id, o.code AS outcome
       FROM session_judges.verdicts v
       JOIN session_judges.outcomes o ON o.id = v.outcome_id
      ORDER BY v.session_id, v.judged_at DESC, v.id DESC
 )
 SELECT n.session_id,
        n.title,
        lv.outcome,
        n.last_seen,
        CASE WHEN pe.embedding IS NOT NULL
             THEN (1.0 - (n.embedding <=> pe.embedding))::float8
        END AS similarity_to_centroid,
        n.source,
        COALESCE(sk.topics, '{}') AS topics
   FROM node_set n
   JOIN sessions.sessions s2 ON s2.session_uuid = n.session_id
   LEFT JOIN latest_verdict lv ON lv.session_id = s2.id
   LEFT JOIN memory.session_knowledge sk ON sk.session_id = n.session_id
   LEFT JOIN memory.project_embeddings pe ON pe.project_id = 313870
  ORDER BY n.last_seen DESC;
