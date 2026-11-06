SELECT COUNT(*)::bigint
FROM requests.diagnostics rd
INNER JOIN requests.requests r ON r.id = rd.request_id
LEFT JOIN sessions.sessions s ON s.id = r.session_id
LEFT JOIN projects.projects p ON p.id = s.project_id
WHERE (NULL::text IS NULL OR s.session_uuid::text = NULL::text)
  AND ('temet-proxy-wt-coord'::text IS NULL OR p.project_path ILIKE '%' || 'temet-proxy-wt-coord' || '%');
