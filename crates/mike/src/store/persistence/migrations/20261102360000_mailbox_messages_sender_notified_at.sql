-- When mike settled whether to tell a message's sender it was not delivered: the notice was
-- sent, or declined because the recipient's launch had closed. NULL means not yet settled,
-- so the mailbox reaper visits each undelivered message once instead of every tick.
ALTER TABLE mailbox.messages ADD COLUMN IF NOT EXISTS sender_notified_at timestamptz;

COMMENT ON COLUMN mailbox.messages.sender_notified_at IS
    'When the sender''s not-delivered notice for this message was sent or declined; NULL until then.';

-- Messages whose sender already holds a notice keep the time of that first notice.
UPDATE mailbox.messages m
   SET sender_notified_at = told.first_notice_at
  FROM (
      SELECT substring(n.body FROM '^\[mike mail\] Not delivered: your message ([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}) ')::uuid
                 AS message_uuid,
             n.to_session_id AS sender_id,
             min(n.queued_at) AS first_notice_at
        FROM mailbox.messages n
       WHERE n.from_session_id IS NULL
         AND n.body LIKE '[mike mail] Not delivered: your message %'
       GROUP BY 1, 2
  ) told
 WHERE m.message_uuid = told.message_uuid
   AND m.from_session_id = told.sender_id
   AND m.sender_notified_at IS NULL;
