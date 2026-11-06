WITH input AS (
    SELECT DISTINCT ON (session_id, kind, subject) *
      FROM unnest($1::bigint[], $2::text[], $3::text[], $4::timestamptz[], $5::text[])
           AS i(session_id, kind, subject, since, message)
     ORDER BY session_id, kind, subject
)
INSERT INTO agents.work_item_attention (session_id, kind, subject, since, message)
SELECT session_id, kind, subject, since, message FROM input
ON CONFLICT (session_id, kind, subject) DO SELECT
RETURNING session_id AS "session_id!", kind AS "kind!", subject AS "subject!",
          id AS "id!", (old.id IS NULL) AS "inserted!",
          (delivered_message_uuid IS NOT NULL) AS "delivered!",
          delivery_attempts AS "delivery_attempts!",
          last_delivery_attempt_at AS "last_delivery_attempt_at?",
          disposition AS "disposition!"
