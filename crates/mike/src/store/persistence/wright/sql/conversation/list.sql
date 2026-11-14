SELECT m.id, m.author, m.body, m.reply_to, m.links, m.created_at,
       s.id AS steer_id, s.change AS steer_change, s.applied_at AS steer_applied_at,
       s.undone_at AS steer_undone_at, s.version AS steer_version, s.refusal AS steer_refusal
  FROM wright.messages m
  LEFT JOIN wright.steers s ON s.message_id = m.id
 WHERE m.id > $1
 ORDER BY m.id
 LIMIT $2
