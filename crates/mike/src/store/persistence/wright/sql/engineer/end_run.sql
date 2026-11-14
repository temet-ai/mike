UPDATE wright.investigations
   SET state = $2, ended_at = now(), exit_code = $3, end_reason = $4
 WHERE id = $1 AND state IN ('starting', 'running')
RETURNING id
