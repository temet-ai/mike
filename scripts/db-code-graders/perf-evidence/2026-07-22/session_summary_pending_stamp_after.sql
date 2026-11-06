UPDATE sessions.sessions
 SET last_seen   = GREATEST(last_seen, NOW()),
     last_status = 'Pending'
 WHERE id = 574994
