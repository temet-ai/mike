SELECT m.message_uuid
  FROM mailbox.messages m
 WHERE m.message_uuid = ANY($1)
   AND m.sender_notified_at IS NOT NULL
 ORDER BY m.id
