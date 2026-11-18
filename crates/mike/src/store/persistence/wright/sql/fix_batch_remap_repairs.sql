UPDATE wright.repairs r
   SET commit_sha = m.sha, updated_at = now()
  FROM unnest($2::bigint[], $3::text[]) AS m(id, sha)
 WHERE r.id = m.id AND r.batch_id = $1::bigint AND r.state = 'batched'
RETURNING r.id
