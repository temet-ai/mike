WITH target AS (
    SELECT r.session_id, r.lease_expires_at
      FROM workspace.session_registrations r
      JOIN remote_control.channels c ON c.session_id = r.session_id
     WHERE c.cse_id = $1
       AND r.released_at IS NULL
), renewed AS (
    UPDATE workspace.session_registrations r
       SET lease_expires_at = now() + make_interval(mins => $2::int)
      FROM target t
     WHERE r.session_id = t.session_id
       AND r.released_at IS NULL
       AND t.lease_expires_at < now() + make_interval(mins => $3::int)
 RETURNING r.session_id, r.lease_expires_at
)
SELECT t.session_id AS "session_id!",
       COALESCE(n.lease_expires_at, t.lease_expires_at) AS "lease_expires_at!"
  FROM target t
  LEFT JOIN renewed n ON n.session_id = t.session_id
