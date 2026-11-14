SELECT id, author, body, client_msg_id, reply_to, links, created_at
  FROM wright.messages
 WHERE id <= $1
 ORDER BY id DESC
 LIMIT $2
