SELECT
                       s.session_uuid     AS "session_uuid!",
                       r.cwd              AS "cwd!",
                       r.worktree         AS "worktree?",
                       r.registered_via   AS "registered_via!",
                       r.registered_at    AS "registered_at!",
                       r.lease_expires_at AS "lease_expires_at!",
                       r.released_at      AS "released_at?",
                       sg.goal            AS "goal?",
                       sg.next_steps      AS "next_steps?",
                       sg.display_name    AS "display_name?",
                       sg.role            AS "role?",
                       sg.instructions    AS "instructions?",
                       CASE
                           WHEN r.released_at IS NOT NULL OR act.lifecycle IN ('ended', 'failed') THEN 'ended'
                           WHEN act.last_request_at IS NULL AND r.registered_at < now()-interval '10 minutes' THEN 'uncaptured'
                           WHEN r.lease_expires_at > now() OR act.last_request_at > now()-interval '15 minutes' THEN 'live'
                           ELSE 'stale'
                       END                AS "liveness!",
                       act.last_request_at       AS "last_request_at?",
                       act.newest_request_number AS "newest_request_number?"
                   FROM workspace.session_registrations r
                   JOIN sessions.sessions s ON s.id = r.session_id
                   LEFT JOIN sessions.session_goals sg ON sg.session_id = r.session_id
                   LEFT JOIN runtime.session_facts f ON f.session_id = r.session_id
                   LEFT JOIN LATERAL (
                       SELECT rq.created_at, rq.request_number
                         FROM requests.requests rq
                        WHERE rq.session_id = r.session_id AND f.session_id IS NULL
                        ORDER BY rq.created_at DESC, rq.id DESC
                        LIMIT 1
                   ) latest ON true
                   CROSS JOIN LATERAL (
                       SELECT COALESCE(f.observed_at, latest.created_at) AS last_request_at,
                              latest.request_number AS newest_request_number,
                              f.lifecycle
                   ) act
                  WHERE r.project_id = 1
                    AND (false OR (r.released_at IS NULL AND COALESCE(act.lifecycle, '') NOT IN ('ended', 'failed')))
                    AND (false OR (
                            COALESCE(act.lifecycle, '') NOT IN ('ended', 'failed')
                            AND NOT (act.last_request_at IS NULL AND r.registered_at < now()-interval '10 minutes')
                            AND (r.lease_expires_at > now() OR act.last_request_at > now()-interval '15 minutes')
                        ))
                  ORDER BY r.registered_at DESC