SELECT rd.category, COUNT(*)::bigint AS total
FROM requests.diagnostics rd
LEFT JOIN requests.requests r ON r.id = rd.request_id
LEFT JOIN sessions.sessions s ON s.id = r.session_id
LEFT JOIN projects.projects p ON p.id = s.project_id
WHERE rd.created_at > NOW() - INTERVAL '24 hours'
  AND ('temet-proxy'::text IS NULL OR p.project_path ILIKE '%' || 'temet-proxy' || '%')
GROUP BY rd.category;
