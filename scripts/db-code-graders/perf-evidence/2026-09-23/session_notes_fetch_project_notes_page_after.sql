SELECT n.note_uuid,
       s.session_uuid  AS "session_uuid!",
       sg.display_name,
       sg.role,
       n.kind, n.title, n.body, n.status, n.tags, n.data,
       n.created_at, n.updated_at
     FROM workspace.session_notes n
     JOIN sessions.sessions s ON s.id = n.session_id
LEFT JOIN sessions.session_goals sg ON sg.session_id = s.id
    WHERE s.project_id = 1
      AND ('observer'::text IS NULL OR lower(sg.role) = lower('observer'::text))
      AND (NULL::text IS NULL OR lower(sg.display_name) = lower(NULL::text))
      AND (NULL::text IS NULL OR n.kind = NULL::text)
      AND (NULL::text IS NULL OR n.status = NULL::text)
      AND (NULL::text IS NULL OR n.tags @> ARRAY[NULL::text]::text[])
      AND (NULL::uuid IS NULL OR s.session_uuid = NULL::uuid)
      AND (NULL::text IS NULL
           OR n.title ILIKE '%' || NULL::text || '%'
           OR n.body  ILIKE '%' || NULL::text || '%')
    ORDER BY n.created_at DESC
    LIMIT 50 OFFSET 0
