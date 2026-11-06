UPDATE mailbox.messages
   SET claimed_at = NULL
 WHERE message_uuid = '3d5648ef-a489-46e4-8892-19aac1f7f1e7'
   AND sent_request_id IS NULL;
