UPDATE wright.investigations
   SET state = 'running', pid = $2, started_at = now()
 WHERE id = $1 AND state = 'starting'
RETURNING id
