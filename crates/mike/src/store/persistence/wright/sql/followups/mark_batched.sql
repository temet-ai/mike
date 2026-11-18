UPDATE wright.followups
   SET state = 'batched', batch_id = $2::bigint, batch_commit_sha = $3::text, updated_at = now()
 WHERE id = $1::bigint AND state = 'handed_off'
RETURNING id
