SELECT request_id AS "request_id!",
       session_id AS "session_id!",
       created_at AS "created_at!"
  FROM (
      SELECT DISTINCT ON (r.session_id)
             r.id                 AS request_id,
             s.session_uuid::text AS session_id,
             r.created_at,
             COALESCE(r.sent_envelope, r.request_envelope) AS envelope,
             r.sent_message_count,
             r.filtered_request_body,
             r.sent_patch_bytes
        FROM requests.requests r
        JOIN sessions.sessions s ON s.id = r.session_id
       WHERE r.created_at >= $1
         AND r.created_at <  $2
       ORDER BY r.session_id, r.created_at DESC, r.id DESC
  ) latest
 WHERE position(convert_to($3::text, 'UTF8') IN latest.envelope) > 0
    OR position(convert_to($3::text, 'UTF8') IN latest.filtered_request_body) > 0
    OR position(convert_to($3::text, 'UTF8') IN latest.sent_patch_bytes) > 0
    OR EXISTS (
         SELECT 1
           FROM requests.sent_messages sm
           JOIN messages.messages m ON m.id = sm.message_id
          WHERE sm.request_id = latest.request_id
            AND strpos(m.body::text, $3::text) > 0)
    OR (latest.sent_message_count IS NULL
        AND EXISTS (
         SELECT 1
           FROM requests.messages rm
           JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id = latest.request_id
            AND strpos(m.body::text, $3::text) > 0
            AND NOT EXISTS (
                 SELECT 1
                   FROM requests.sent_messages sm
                  WHERE sm.request_id = rm.request_id
                    AND sm.ord = rm.ord)))
    OR EXISTS (
         SELECT 1
           FROM requests.deltas d
          WHERE d.request_id = latest.request_id
            AND d.sent_patch_bytes IS NOT NULL
            AND position(convert_to($3::text, 'UTF8') IN d.sent_patch_bytes) > 0)
 ORDER BY latest.created_at DESC
 LIMIT $4
