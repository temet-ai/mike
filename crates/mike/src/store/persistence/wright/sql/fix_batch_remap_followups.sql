UPDATE wright.followups f
   SET batch_commit_sha = m.sha, updated_at = now()
  FROM unnest($2::bigint[], $3::text[]) AS m(id, sha)
 WHERE f.id = m.id AND f.batch_id = $1::bigint AND f.state = 'batched'
RETURNING f.id
