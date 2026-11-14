SELECT id, author, body, client_msg_id, reply_to, links, created_at
  FROM wright.messages
 WHERE author = 'operator' AND client_msg_id = $1
