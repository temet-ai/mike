SELECT DISTINCT ON (l.subject_id)
       l.subject_id AS "end_turn_id!", t.session_id AS "session_id!", l.id AS "label_id!",
       l.label AS "label!", l.tier AS "tier!", t.closed_at,
       s.session_uuid::text AS "session_uuid!"
  FROM eval.labels_as_of($1) l
  JOIN memory.operator_turns t ON t.id = l.subject_id
  JOIN sessions.sessions s ON s.id = t.session_id
 WHERE l.subject_kind = 'session_end'
   AND l.axis = 'session_end'
   AND l.tier = ANY($2::text[])
   AND (l.tier = 'gold' OR l.source = $3)
 ORDER BY l.subject_id,
          CASE l.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2 WHEN 'seed' THEN 3 ELSE 4 END,
          l.created_at DESC, l.id DESC
