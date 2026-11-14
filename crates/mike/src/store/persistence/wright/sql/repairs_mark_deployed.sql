UPDATE wright.repairs
   SET state = 'deployed', deployed_at = now(), updated_at = now()
 WHERE id = ANY ($1::bigint[]) AND state = 'landed'
RETURNING id
