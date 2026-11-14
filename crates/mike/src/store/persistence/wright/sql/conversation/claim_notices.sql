UPDATE wright.messages
   SET notice_sent_at = now()
 WHERE needs_decision AND notice_sent_at IS NULL
RETURNING id
