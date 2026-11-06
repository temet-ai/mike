UPDATE mailbox.messages
   SET claimed_at = now()
 WHERE message_uuid = '3d5648ef-a489-46e4-8892-19aac1f7f1e7'
   AND claimed_at IS NULL
   AND consumed_at IS NULL
   AND dead_lettered_at IS NULL;
