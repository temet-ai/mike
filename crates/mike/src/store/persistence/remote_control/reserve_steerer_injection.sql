-- Claim `steering.interventions.pushed_at` and insert the provenance row in
-- one statement — the steerer-source counterpart of reserve_injection.sql's
-- mailbox claim. `created_at` is returned (not `pushed_at` itself, which
-- isn't a column of the table being inserted into) as the reservation's
-- timestamp: `now()` is stable for the whole statement (transaction
-- timestamp semantics), so the row's own `created_at` DEFAULT and the
-- claim's `pushed_at = now()` above evaluate to the identical value. The
-- caller uses it to arm the emitted-but-unacked fallback timer
-- (`release_stale_steerer_push.sql`) against the exact instant this
-- reservation stamped.
WITH recipient_channel AS (
    SELECT c.id AS channel_id, s.id AS session_id
      FROM remote_control.channels c
      JOIN sessions.sessions s ON s.session_uuid = c.local_session_uuid
     WHERE c.cse_id = $1 AND s.session_uuid = $2
), claimed AS (
    UPDATE steering.interventions i SET pushed_at = now()
      FROM recipient_channel c
     WHERE i.uuid = $3 AND i.session_id = c.session_id
       AND i.pushed_at IS NULL AND i.acked_at IS NULL AND i.reverted_at IS NULL
       AND (i.expires_at IS NULL OR i.expires_at > now())
    RETURNING i.uuid, c.channel_id
)
INSERT INTO remote_control.injections(event_id, channel_id, message_uuid, source_kind, source_id, frame)
SELECT $4, channel_id, NULL, 'steerer', uuid::text, $5 FROM claimed
RETURNING event_id AS "event_id!", created_at AS "created_at!"
