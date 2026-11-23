DELETE FROM sessions.sessions WHERE id IN (
    SELECT s.id FROM sessions.sessions s
    JOIN runtime.session_identities i ON i.session_id = s.id
    WHERE s.last_seen < $1::timestamptz - interval '90 days' AND NOT s.pinned
      AND NOT EXISTS (SELECT 1 FROM runtime.capture_exchanges e WHERE e.session_id = s.id)
    ORDER BY s.last_seen LIMIT $2
)
