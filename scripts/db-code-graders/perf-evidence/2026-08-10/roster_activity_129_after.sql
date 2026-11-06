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
       CASE
           WHEN r.released_at IS NOT NULL THEN 'ended'
           WHEN r.lease_expires_at > now() THEN 'live'
           ELSE 'stale'
       END                AS "liveness!",
       act.last_request_at       AS "last_request_at?",
       act.newest_request_number AS "newest_request_number?"
   FROM workspace.session_registrations r
   JOIN sessions.sessions s ON s.id = r.session_id
   LEFT JOIN sessions.session_goals sg ON sg.session_id = r.session_id
   -- LATERAL rides idx_requests_session_created_id backward, LIMIT 1 per
   -- registration — activity truth from captured traffic (issue #129), not
   -- a correlated scalar subquery (sql-set-based.md).
   LEFT JOIN LATERAL (
       SELECT rq.created_at     AS last_request_at,
              rq.request_number AS newest_request_number
         FROM requests.requests rq
        WHERE rq.session_id = r.session_id
        ORDER BY rq.created_at DESC, rq.id DESC
        LIMIT 1
   ) act ON true
  WHERE r.project_id = 354827
    AND (false OR r.released_at IS NULL)
    AND (false OR r.lease_expires_at > now() OR act.last_request_at > (now() - interval '15 minutes'))
  ORDER BY r.registered_at DESC;
