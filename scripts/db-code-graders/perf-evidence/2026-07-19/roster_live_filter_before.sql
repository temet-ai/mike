SELECT
    s.session_uuid     AS "session_uuid",
    r.cwd               AS "cwd",
    r.worktree          AS "worktree",
    r.registered_via    AS "registered_via",
    r.registered_at     AS "registered_at",
    r.lease_expires_at  AS "lease_expires_at",
    r.released_at       AS "released_at",
    sg.goal             AS "goal",
    sg.next_steps       AS "next_steps",
    CASE
        WHEN r.released_at IS NOT NULL THEN 'ended'
        WHEN r.lease_expires_at > now() THEN 'live'
        ELSE 'stale'
    END                AS "liveness"
FROM workspace.session_registrations r
JOIN sessions.sessions s ON s.id = r.session_id
LEFT JOIN sessions.session_goals sg ON sg.session_id = r.session_id
WHERE r.project_id = (SELECT id FROM projects.projects WHERE project_path = 'projects/temet/temet-proxy' LIMIT 1)
ORDER BY r.registered_at DESC;
