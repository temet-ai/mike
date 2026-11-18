UPDATE wright.followups
   SET state = 'filed', batch_id = NULL, batch_commit_sha = NULL,
       fixer_session = NULL, fixer_machine = NULL, updated_at = now()
 WHERE batch_id = $1::bigint AND id = ANY ($2::bigint[]) AND state = 'batched'
RETURNING id
