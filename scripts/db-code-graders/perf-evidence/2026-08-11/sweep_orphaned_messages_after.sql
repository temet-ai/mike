WITH candidates AS (
    SELECT m.id
    FROM messages.messages m
    WHERE NOT EXISTS (
              SELECT 1 FROM requests.messages rm WHERE rm.message_id = m.id
          )
      AND NOT EXISTS (
              SELECT 1 FROM requests.deltas rd WHERE rd.message_id = m.id
          )
    ORDER BY m.last_seen ASC
    LIMIT 1000
)
DELETE FROM messages.messages m
USING candidates c
WHERE m.id = c.id;
