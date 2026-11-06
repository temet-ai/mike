WITH anchor AS (
     SELECT se.embedding, se.session_id, s.project_id
       FROM memory.session_embeddings se
       JOIN sessions.sessions s ON s.session_uuid = se.session_id
      WHERE se.session_id = '1c59286c-6b3e-4f72-98ef-03e2b4c60695'
 ),
 latest_verdict AS (
     SELECT DISTINCT ON (v.session_id) v.session_id, o.code AS outcome
       FROM session_judges.verdicts v
       JOIN session_judges.outcomes o ON o.id = v.outcome_id
      ORDER BY v.session_id, v.judged_at DESC, v.id DESC
 )
 SELECT b.session_id,
        s.title,
        s.project_id,
        p.project_path,
        lv.outcome,
        s.last_seen,
        (1.0 - (a.embedding <=> b.embedding))::float8 AS similarity,
        b.source,
        COALESCE(sk.topics, '{}') AS topics
   FROM anchor a
   JOIN memory.session_embeddings b ON b.session_id <> a.session_id
   JOIN sessions.sessions s ON s.session_uuid = b.session_id
   JOIN projects.projects p ON p.id = s.project_id
   LEFT JOIN latest_verdict lv ON lv.session_id = s.id
   LEFT JOIN memory.session_knowledge sk ON sk.session_id = b.session_id
  WHERE (NOT false::boolean OR s.project_id = a.project_id)
    AND (1.0 - (a.embedding <=> b.embedding)) >= 0.6
  ORDER BY a.embedding <=> b.embedding ASC
  LIMIT 8;
