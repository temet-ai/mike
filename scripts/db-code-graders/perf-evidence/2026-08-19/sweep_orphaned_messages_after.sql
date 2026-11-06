WITH candidates AS (
    SELECT id FROM messages.messages
    ORDER BY last_seen ASC
    LIMIT 2000
)
DELETE FROM messages.messages m
USING candidates c
WHERE m.id = c.id
  AND NOT EXISTS (
      SELECT 1 FROM requests.messages rm WHERE rm.message_id = m.id
  )
  AND NOT EXISTS (
      SELECT 1 FROM requests.deltas rd WHERE rd.message_id = m.id
  )
