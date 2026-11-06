SELECT rd.request_id,
       s.session_uuid::text AS session_id,
       rd.category,
       rd.analyzer,
       rd.advice,
       rd.matched_text,
       rd.match_range,
       rd.other_hits,
       rd.created_at,
       r.model            AS request_model,
       rs.code            AS request_status,
       r.path             AS request_path,
       r.human_prompt     AS request_human_prompt,
       r.created_at       AS request_created_at,
       0::bigint AS total_rows,
       p.project_path     AS project_path
FROM requests.diagnostics rd
INNER JOIN requests.requests r ON r.id = rd.request_id
INNER JOIN requests.statuses rs ON rs.id = r.status_id
LEFT JOIN sessions.sessions s ON s.id = r.session_id
LEFT JOIN projects.projects p ON p.id = s.project_id
WHERE (NULL::text IS NULL OR s.session_uuid::text = NULL::text)
  AND ('temet-proxy'::text IS NULL OR p.project_path ILIKE '%' || 'temet-proxy' || '%')
ORDER BY rd.created_at DESC
LIMIT 50 OFFSET 0;
