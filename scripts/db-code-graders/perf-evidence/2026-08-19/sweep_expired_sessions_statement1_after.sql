DELETE FROM sessions.sessions s
USING (
    SELECT s2.id
    FROM sessions.sessions s2
    LEFT JOIN projects.projects p ON p.id = s2.project_id
    LEFT JOIN (
        SELECT DISTINCT r.session_id
        FROM requests.requests r
        JOIN requests.statuses rs ON rs.id = r.status_id
                                 AND rs.code IN ('pending', 'streaming')
        WHERE r.created_at >= now()::timestamptz - '365 days'::interval
    ) inflight ON inflight.session_id = s2.id
    WHERE NOT s2.pinned
      AND s2.ttl_override IS NULL
      AND COALESCE(p.pinned, FALSE) = FALSE
      AND p.ttl_override IS NULL
      AND s2.last_seen < now()::timestamptz - '365 days'::interval
      AND inflight.session_id IS NULL
) sub
WHERE s.id = sub.id
